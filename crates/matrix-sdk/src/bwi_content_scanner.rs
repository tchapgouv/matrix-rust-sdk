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

use ruma::events::room::MediaSource;

use crate::bwi_content_scanner::ScanState::Infected;
use crate::media::MediaFileHandle;
use crate::Error::InconsistentState;
use crate::{Client, Result};

#[derive(Debug, Clone)]
#[allow(unused_variables, dead_code, missing_docs, clippy::all)]
pub enum ScanState {
    Trusted,
    Infected,
    Error,
    InProgress,
    NotFound,
}

#[derive(Debug, Clone)]
#[allow(unused_variables, dead_code, missing_docs, clippy::all, clippy::unused_async)]
pub struct BWIContentScanner {
    /// The underlying HTTP client.
    client: Client,
}

#[allow(unused_variables, missing_docs, clippy::all, clippy::unused_async)]
impl BWIContentScanner {
    pub(crate) fn new(client: Client) -> Self {
        Self { client }
    }
    pub fn set_content_scanner_url(&self, url: String) {}
    pub async fn get_content_scanner_result_for_attachment(
        &self,
        media_source: Arc<MediaSource>,
    ) -> Result<ScanState> {
        Ok(Infected)
    }

    pub async fn download_attachment_from_content_scanner(
        &self,
        media_source: Arc<MediaSource>,
        body: Option<String>,
        mime_type: String,
        use_cache: bool,
        temp_dir: Option<String>,
    ) -> Result<MediaFileHandle> {
        Err(InconsistentState)
    }
}
