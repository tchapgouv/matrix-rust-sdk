/*
 * Copyright (c) 2024 BWI GmbH
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
use std::sync::Arc;
use anyhow::anyhow;
use ruma::events::room::MediaSource;
use crate::bwi_client_extensions::BWIScanState::Infected;
use crate::client::Client;
use crate::error::ClientError;

#[derive(Clone, uniffi::Enum)]
pub enum BWIScanState {
    Trusted,
    Infected,
    Error,
    InProgress,
    NotFound,
}
#[uniffi::export(async_runtime = "tokio")]
impl Client {
    pub fn bwi_set_content_scanner_url(&self, url: String) {
        //self.inner.
    }

    pub async fn bwi_get_content_scanner_result_for_attachment(
        &self,
        media_source: Arc<MediaSource>,
    ) -> Result<BWIScanState, ClientError> {
        Ok(Infected)
    }

    pub async fn bwi_download_attachment_from_content_scanner(
        &self,
        media_source: Arc<MediaSource>,
    ) -> Result<Vec<u8>, ClientError> {
        Err(anyhow!("This method is not implemented, but your file is infected anyway!").into())
    }
}