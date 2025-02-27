/*
 * Copyright (c) 2025 BWI GmbH
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
pub mod dto;
pub mod request;
mod url;

use crate::content_scanner::dto::{
    BWIContentScannerPublicKey, BWIPublicKeyDto, BWIScanErrorResultDto, BWIScanStateResultDto,
    EncryptedMetadataRequestBuilder,
};
use crate::content_scanner::url::BWIContentScannerUrl;
use crate::content_scanner::BWIContentScannerError::{PublicKeyNotAvailable, PublicKeyParseFailed};
use http::StatusCode;
use matrix_sdk_base::ruma::events::room::EncryptedFile;
use matrix_sdk_base_bwi::content_scanner::scan_state::BWIScanState;
use matrix_sdk_base_bwi::http_client::HttpError;
use matrix_sdk_base_bwi::http_client::HttpError::{Failed, NotFound};
use request::download_encrypted::v1::Request as DownloadRequest;
use request::scan_encrypted::v1::Request as ScanRequest;
use reqwest::Response;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, warn};

#[derive(Error, Debug, Eq, PartialEq)]
pub enum BWIContentScannerError {
    #[error("Public key for content scanner is not available")]
    PublicKeyNotAvailable(#[from] HttpError),
    #[error("Public key could not parsed")]
    PublicKeyParseFailed,
    #[error("There was an error in the response of the content scanner")]
    ResponseError(#[from] ResponseError),
    #[error("Scan failed")]
    ScanFailed,
    #[error("Scan response could not be parsed")]
    ScanResponseParseFailed,
    #[error("Download failed")]
    DownloadFailed,
}

#[derive(Error, Debug, Eq, PartialEq)]
pub enum ResponseError {
    #[error("The status code does not match with the body")]
    InconsistentResponse,
    #[error("The response was malformed")]
    UnableToParseResponse,
}

pub enum ReasonForForbiddenResponse {
    MediaInfected,
    MimeTypeForbidden,
    DecryptionFailed,
}

/// map the magic String from https://github.com/element-hq/matrix-content-scanner-python/blob/main/docs/api.md for status code 403 to enum entries
fn map_forbidden_reason_string_to_enum(
    reason_as_string: &str,
) -> Result<ReasonForForbiddenResponse, ResponseError> {
    match reason_as_string {
        "MCS_MEDIA_NOT_CLEAN" => Ok(ReasonForForbiddenResponse::MediaInfected),
        "MCS_MIME_TYPE_FORBIDDEN" => Ok(ReasonForForbiddenResponse::MimeTypeForbidden),
        "MCS_BAD_DECRYPTION" => Ok(ReasonForForbiddenResponse::DecryptionFailed),
        _ => Err(ResponseError::UnableToParseResponse),
    }
}

#[derive(Clone, Debug)]
pub struct BWIScannedMedia {
    pub scanned_media: Arc<Mutex<HashMap<String, BWIScanState>>>,
}

impl BWIScannedMedia {
    fn new() -> Self {
        Self { scanned_media: Arc::new(Mutex::new(HashMap::new())) }
    }
}

#[derive(Clone)]
pub struct BWIContentScanner {
    content_scanner_url: BWIContentScannerUrl,
    http_client: reqwest::Client,
    scanned_media: BWIScannedMedia,
}

impl Debug for BWIContentScanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BWIContentScanner")
            .field("content_scanner_url", &self.content_scanner_url)
            .finish()
    }
}

impl BWIContentScanner {
    fn new(
        http_client: reqwest::Client,
        content_scanner_url: BWIContentScannerUrl,
        scanned_media: BWIScannedMedia,
    ) -> Self {
        Self { http_client, content_scanner_url, scanned_media }
    }

    pub fn new_with_url_as_str(
        http_client: reqwest::Client,
        content_scanner_url: &str,
    ) -> Result<Self, ::url::ParseError> {
        let content_scanner_url =
            BWIContentScannerUrl::for_base_url_as_string(content_scanner_url)?;
        Ok(Self::new(http_client, content_scanner_url, BWIScannedMedia::new()))
    }

    pub fn new_with_url(http_client: &reqwest::Client, content_scanner_url: &::url::Url) -> Self {
        let content_scanner_url =
            BWIContentScannerUrl::for_base_url(content_scanner_url.to_owned());
        Self::new(http_client.to_owned(), content_scanner_url, BWIScannedMedia::new())
    }

    pub fn get_scanned_media(&self) -> &'_ BWIScannedMedia {
        &self.scanned_media
    }

    pub async fn get_public_key(
        &self,
    ) -> Result<BWIContentScannerPublicKey, BWIContentScannerError> {
        let public_key_response = self.send_get_public_key_request().await?;
        match public_key_response.status() {
            StatusCode::OK => {
                Ok(self.handle_public_key_success_response(public_key_response).await?)
            }
            StatusCode::NOT_FOUND => Err(PublicKeyNotAvailable(NotFound)),
            status_code => Err(PublicKeyNotAvailable(Failed(status_code.into()))),
        }
    }

    async fn send_get_public_key_request(&self) -> Result<Response, BWIContentScannerError> {
        self.http_client.get(self.content_scanner_url.get_public_key_url()).send().await.map_err(
            |e| {
                error!("Failed to send get public key request: {:?}", e);
                PublicKeyNotAvailable(Failed(e.status().unwrap().as_u16()))
            },
        )
    }

    async fn handle_public_key_success_response(
        &self,
        public_key_response: Response,
    ) -> Result<BWIContentScannerPublicKey, BWIContentScannerError> {
        Ok(BWIContentScannerPublicKey(
            public_key_response
                .json::<BWIPublicKeyDto>()
                .await
                .map_err(|_| PublicKeyParseFailed)?
                .public_key,
        ))
    }

    pub async fn create_download_media_request(
        &self,
        file: &EncryptedFile,
    ) -> Result<DownloadRequest, BWIContentScannerError> {
        let public_key = self.get_public_key().await?;
        let encrypted_metadata = EncryptedMetadataRequestBuilder::for_encrypted_file(file)
            .build_encrypted_request(&public_key)
            .map_err(|_| BWIContentScannerError::DownloadFailed)?;
        debug!("###BWI### Downloading authenticated media with url {:?}", file.url);
        Ok(DownloadRequest::from_encrypted_metadata(encrypted_metadata))
    }

    pub async fn create_scan_media_request(
        &self,
        file: &EncryptedFile,
    ) -> Result<ScanRequest, BWIContentScannerError> {
        let public_key = self.get_public_key().await?;
        let encrypted_metadata = EncryptedMetadataRequestBuilder::for_encrypted_file(file)
            .build_encrypted_request(&public_key)
            .map_err(|_| BWIContentScannerError::DownloadFailed)?;
        debug!("###BWI### Downloading authenticated media with url {:?}", file.url);
        Ok(ScanRequest::from_encrypted_metadata(encrypted_metadata))
    }

    pub fn map_success_to_state(body: BWIScanStateResultDto) -> BWIScanState {
        match body.clean {
            true => BWIScanState::Trusted,
            false => {
                warn!("###BWI### inconsistent response from the content scanner. Maybe an old version of the content scanner ist used");
                BWIScanState::Infected
            }
        }
    }

    pub fn map_forbidden_reason_to_scan_state(
        body: &BWIScanErrorResultDto,
    ) -> Result<BWIScanState, ResponseError> {
        debug!("###BWI### Map forbidden error response {:?}", &body);
        match map_forbidden_reason_string_to_enum(&body.reason)? {
            ReasonForForbiddenResponse::MediaInfected => Ok(BWIScanState::Infected),
            ReasonForForbiddenResponse::MimeTypeForbidden => Ok(BWIScanState::MimeTypeNotAllowed),
            ReasonForForbiddenResponse::DecryptionFailed => Ok(BWIScanState::Error),
        }
    }
}
