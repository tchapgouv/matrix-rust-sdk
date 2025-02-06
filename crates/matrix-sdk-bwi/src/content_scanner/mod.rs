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
mod dto;
mod request;
mod url;

use crate::content_scanner::dto::{
    BWIContentScannerPublicKey, BWIPublicKeyDto, BWIScanStateResultDto,
    EncryptedMetadataRequestBuilder,
};
use crate::content_scanner::url::BWIContentScannerUrl;
use crate::content_scanner::BWIContentScannerError::ScanFailed;
use http::StatusCode;
use matrix_sdk_base::ruma::events::room::MediaSource::{Encrypted, Plain};
use matrix_sdk_base::ruma::events::room::{EncryptedFile, MediaSource};
use matrix_sdk_base_bwi::content_scanner::scan_state::BWIScanState;
use matrix_sdk_base_bwi::http_client::HttpError;
use reqwest::{Error, Response};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::content_scanner::request::v1::download_encrypted::Request;
use tracing::{debug, error, warn};

#[derive(Debug)]
pub enum BWIContentScannerError {
    PublicKeyNotAvailable(HttpError),
    PublicKeyParseError,
    ScanFailed,
    DownloadFailed,
}

#[derive(Clone)]
struct BWIScannedMedia {
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

    pub async fn get_public_key(
        &self,
    ) -> Result<BWIContentScannerPublicKey, BWIContentScannerError> {
        let public_key = self
            .send_get_public_key_request()
            .await?
            .json::<BWIPublicKeyDto>()
            .await
            .map_err(|_| BWIContentScannerError::PublicKeyParseError)?;
        Ok(BWIContentScannerPublicKey(public_key.public_key))
    }

    async fn send_get_public_key_request(&self) -> Result<Response, BWIContentScannerError> {
        self.http_client.get(self.content_scanner_url.get_public_key_url()).send().await.map_err(
            |e| {
                error!("Failed to send get public key request: {:?}", e);
                BWIContentScannerError::PublicKeyNotAvailable(HttpError::Failed(
                    e.status().unwrap().as_u16(),
                ))
            },
        )
    }

    pub async fn scan_attachment(&self, media_source: MediaSource) -> BWIScanState {
        match media_source {
            Encrypted(encrypted_file) => self.scan_encrypted_attachment(encrypted_file).await,
            Plain(attachment) => {
                debug!("###BWI### All media should be encrypted. This should be a local echo with uri {:?}", attachment);
                BWIScanState::InProgress
            }
        }
    }

    async fn scan_encrypted_attachment(&self, encrypted_file: Box<EncryptedFile>) -> BWIScanState {
        let mut guard = self.scanned_media.scanned_media.lock().await;

        let media_uri = encrypted_file.url.to_string();
        let optional_previous_scan_state = guard.get(&media_uri);

        match optional_previous_scan_state {
            None | Some(BWIScanState::Error) => {
                let scan_result = self
                    .trigger_scan_request_for_encrypted_media(encrypted_file)
                    .await
                    .unwrap_or(BWIScanState::Error);
                guard.insert(media_uri.clone(), scan_result.clone());
                scan_result
            }
            Some(previous_scan_state) => previous_scan_state.clone(),
        }
    }

    async fn trigger_scan_request_for_encrypted_media(
        &self,
        encrypted_media: Box<EncryptedFile>,
    ) -> Result<BWIScanState, BWIContentScannerError> {
        debug!(
            "###BWI### media with uri {:?} is scanned via the content scanner",
            encrypted_media.url
        );
        let public_key = self.get_public_key().await?;
        let scan_result = self.send_scan_request(encrypted_media, &public_key).await?;
        match scan_result {
            Ok(response) => self.handle_scan_response(response).await,
            Err(_error) => {
                error!("###BWI### Failed to encrypt media");
                Err(ScanFailed)
            }
        }
    }

    async fn send_scan_request(
        &self,
        encrypted_media: Box<EncryptedFile>,
        public_key: &BWIContentScannerPublicKey,
    ) -> Result<Result<Response, Error>, BWIContentScannerError> {
        Ok(self
            .http_client
            .post(self.content_scanner_url.get_scan_url())
            .json(
                &EncryptedMetadataRequestBuilder::for_encrypted_file(*encrypted_media)
                    .build_encrypted_request(public_key)
                    .map_err(|_| ScanFailed)?,
            )
            .send()
            .await)
    }

    /// Map the responses to the given semantic used by NV
    /// https://github.com/element-hq/matrix-content-scanner-python/blob/main/docs/api.md
    async fn handle_scan_response(
        &self,
        response: Response,
    ) -> Result<BWIScanState, BWIContentScannerError> {
        let status = response.status();
        debug!("###BWI### Scan finished with status {:?}", &status);

        match status {
            StatusCode::OK => Self::handle_ok_response(response).await,
            StatusCode::FORBIDDEN => Self::handle_forbidden_response(response).await,
            StatusCode::NOT_FOUND => Ok(BWIScanState::NotFound),
            _ => Err(ScanFailed),
        }
    }

    async fn handle_forbidden_response(
        response: Response,
    ) -> Result<BWIScanState, BWIContentScannerError> {
        let body = response.json::<BWIScanStateResultDto>().await.map_err(|_| ScanFailed)?;
        let scan_result = match body.clean {
            true => {
                warn!("###BWI### inconsistent response from the content scanner. Is is forbidden but clean");
                BWIScanState::Error
            }
            false => BWIScanState::Infected,
        };
        Ok(scan_result)
    }

    async fn handle_ok_response(
        response: Response,
    ) -> Result<BWIScanState, BWIContentScannerError> {
        let body = response.json::<BWIScanStateResultDto>().await.map_err(|_| ScanFailed)?;
        let scan_result = match body.clean {
            true => BWIScanState::Trusted,
            false => {
                warn!("###BWI### inconsistent response from the content scanner. Maybe an old version of the content scanner ist used");
                BWIScanState::Infected
            }
        };
        Ok(scan_result)
    }

    pub async fn download_authenticated_media_request(
        &self,
        file: Box<EncryptedFile>,
    ) -> Result<Request, BWIContentScannerError> {
        let public_key = self.get_public_key().await?;
        let encrypted_metadata = EncryptedMetadataRequestBuilder::for_encrypted_file(*file.clone())
            .build_encrypted_request(&public_key)
            .map_err(|_| BWIContentScannerError::DownloadFailed)?;
        debug!("###BWI### Downloading authenticated media with url {:?}", file.url);
        Ok(Request::from_encrypted_metadata(encrypted_metadata))
    }
}
