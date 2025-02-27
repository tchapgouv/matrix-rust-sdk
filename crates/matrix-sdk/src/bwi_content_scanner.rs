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

//! Types and functions related to the content scanner as a compatibility layer for the matrix_sdk

use crate::config::RequestConfig;
use crate::HttpError::Api;
use crate::RumaApiError::Other;
use crate::{Client, HttpError};
use async_trait::async_trait;
use http::StatusCode;
use matrix_sdk_base_bwi::content_scanner::scan_state::BWIScanState;
use matrix_sdk_bwi::content_scanner::dto::BWIScanStateResult;
use matrix_sdk_bwi::content_scanner::request::scan_encrypted::v1::Response as ScanResponse;
use matrix_sdk_bwi::content_scanner::{BWIContentScanner, BWIContentScannerError};
use ruma::api::error::FromHttpResponseError::Server;
use ruma::api::error::MatrixError;
use ruma::api::error::MatrixErrorBody::Json;
use ruma::events::room::EncryptedFile;
use tracing::{debug, error};

/// The content scanner facade for the matrix_sdk layer
#[derive(Clone, Debug)]
pub struct BWIContentScannerWrapper {
    client: Option<Client>, // option is needed, because otherwise would be untestable
}

impl BWIContentScannerWrapper {
    /// Create a new Wrapper for the content scanner
    pub fn new(client: Client) -> Self {
        Self { client: Some(client) }
    }

    /// Only for testing purposes
    #[cfg(feature = "testing")]
    pub fn test_wrapper() -> Self {
        Self { client: None }
    }
}

#[async_trait]
impl BWIScanMediaExt for BWIContentScannerWrapper {
    async fn scan_media(
        &self,
        media_source: &EncryptedFile,
    ) -> Result<BWIScanState, BWIContentScannerError> {
        if let Some(client) = &self.client {
            client.scan_media(media_source).await
        } else {
            panic!("You try to scan something with a test_dummy")
        }
    }
}

/// Extension trait for downloading media files
#[async_trait]
pub trait BWIDownloadMediaExt {
    /// Download authenticated media
    async fn download_authenticated_media(
        &self,
        media_source: &EncryptedFile,
        request_config: Option<RequestConfig>,
    ) -> Result<FileContent, BWIContentScannerError>;

    /// Download unauthenticated media
    #[deprecated(note = "should be replaced with the authenticated method asap")]
    async fn download_unauthenticated_media(
        &self,
        media_source: &EncryptedFile,
    ) -> Result<FileContent, BWIContentScannerError>;
}

/// Extension trait for scanning media files
#[async_trait]
pub trait BWIScanMediaExt {
    /// scan a media file
    async fn scan_media(
        &self,
        media_source: &EncryptedFile,
    ) -> Result<BWIScanState, BWIContentScannerError>;
}

/// The raw bytes of a file
#[derive(Debug)]
pub struct FileContent(Vec<u8>);

impl FileContent {
    pub(crate) fn value(self) -> Vec<u8> {
        self.0
    }
}

#[async_trait]
impl BWIDownloadMediaExt for Client {
    async fn download_authenticated_media(
        &self,
        media_source: &EncryptedFile,
        request_config: Option<RequestConfig>,
    ) -> Result<FileContent, BWIContentScannerError> {
        debug!("###BWI### download authenticated media {:?}", media_source);
        let request = self.content_scanner().create_download_media_request(media_source).await?;
        Ok(self
            .send(request, request_config)
            .await
            .inspect(|response| debug!("###BWI### Response for downloading {:?}", response))
            .map(|response| FileContent(response.file))
            .inspect_err(|e| error!("###BWI### Download failed: {}", e))
            .map_err(|_| BWIContentScannerError::DownloadFailed)?)
    }

    async fn download_unauthenticated_media(
        &self,
        media_source: &EncryptedFile,
    ) -> Result<FileContent, BWIContentScannerError> {
        debug!("###BWI### download unauthenticated media {:?}", media_source);
        let request = self.content_scanner().create_download_media_request(media_source).await?;
        Ok(self
            .send(request, None)
            .await
            .inspect(|response| debug!("###BWI### Response for downloading {:?}", response))
            .map(|response| FileContent(response.file))
            .inspect_err(|e| error!("###BWI### Download failed: {}", e))
            .map_err(|_| BWIContentScannerError::DownloadFailed)?)
    }
}

#[async_trait]
impl BWIScanMediaExt for Client {
    async fn scan_media(
        &self,
        file: &EncryptedFile,
    ) -> Result<BWIScanState, BWIContentScannerError> {
        debug!("###BWI### scan unauthenticated media {:?}", file);
        let content_scanner = self.content_scanner();

        let mut guard = content_scanner.get_scanned_media().scanned_media.lock().await;
        let media_uri = file.url.to_string();
        let optional_previous_scan_state = guard.get(&media_uri);

        match optional_previous_scan_state {
            None | Some(BWIScanState::Error) => {
                let request = content_scanner.create_scan_media_request(file).await?;

                let scan_state = match self.send(request, None).await {
                    Ok(response) => {
                        debug!("###BWI### Response for scan {:?}", response);
                        content_scanner.handle_scan_response(response)
                    }
                    Err(error) => {
                        error!("###BWI### Scan failed: {:?}", error);
                        content_scanner.handle_scan_error(error)
                    }
                };

                guard.insert(media_uri, scan_state.clone());
                Ok(scan_state)
            }
            Some(previous_scan_state) => Ok(previous_scan_state.clone()),
        }
    }
}

trait BWIInnerScanMediaExt {
    fn handle_scan_response(&self, response: ScanResponse) -> BWIScanState;
    fn handle_scan_error(&self, error: HttpError) -> BWIScanState;
}

impl BWIInnerScanMediaExt for BWIContentScanner {
    fn handle_scan_response(&self, response: ScanResponse) -> BWIScanState {
        match BWIScanStateResult::try_from(response) {
            Ok(BWIScanStateResult::Success(success_response)) => {
                Self::map_success_to_state(success_response)
            }
            Ok(BWIScanStateResult::Error(StatusCode::FORBIDDEN, response)) => {
                Self::map_forbidden_reason_to_scan_state(&response).unwrap_or(BWIScanState::Error)
            }
            Ok(BWIScanStateResult::Error(StatusCode::NOT_FOUND, _)) => BWIScanState::NotFound,
            _ => BWIScanState::Error,
        }
    }

    fn handle_scan_error(&self, error: HttpError) -> BWIScanState {
        debug!("###BWI### Scan failed: {}", error);
        if let Ok(BWIScanStateResult::Error(status, response)) = error.try_into() {
            match status {
                StatusCode::FORBIDDEN => Self::map_forbidden_reason_to_scan_state(&response)
                    .unwrap_or(BWIScanState::Error),
                StatusCode::NOT_FOUND => BWIScanState::NotFound,
                _ => BWIScanState::Error,
            }
        } else {
            BWIScanState::Error
        }
    }
}

impl TryFrom<HttpError> for BWIScanStateResult {
    type Error = BWIContentScannerError;

    fn try_from(value: HttpError) -> Result<Self, BWIContentScannerError> {
        if let Api(Server(Other(MatrixError { status_code: status, body: Json(value) }))) = value {
            let value = serde_json::from_value(value)
                .map_err(|_| BWIContentScannerError::ScanResponseParseFailed)?;
            Ok(BWIScanStateResult::Error(status, value))
        } else {
            Err(BWIContentScannerError::ScanResponseParseFailed)
        }
    }
}
