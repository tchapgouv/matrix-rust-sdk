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

//! Extension Traits for the client

use crate::bwi_extensions::attachment::FileSize;
use crate::Error;
use async_trait::async_trait;
use matrix_sdk_bwi::attachment::FILE_SIZE_LIMIT;
use matrix_sdk_bwi::settings_cache::BWISettingsCache;
use tracing::warn;

/// Extension trait for client in order to set it up
#[async_trait]
pub trait BWIClientSetupExt {
    /// Sync all changes and cache them
    async fn sync_settings(&self) -> Result<(), Error>;
}

#[async_trait]
impl BWIClientSetupExt for crate::Client {
    async fn sync_settings(&self) -> Result<(), Error> {
        self.sync_file_size_limit_with_server().await?;
        Ok(())
    }
}

#[async_trait]
trait BWIInnerClientSetup {
    async fn sync_file_size_limit_with_server(&self) -> Result<(), Error>;

    async fn try_load_size_limit_from_server(&self) -> Result<FileSize, Error>;

    async fn save_size_limit(&self, size_limit: &FileSize);
}

#[async_trait]
impl BWIInnerClientSetup for crate::Client {
    async fn sync_file_size_limit_with_server(&self) -> Result<(), Error> {
        let file_size_limit = self.try_load_size_limit_from_server().await?;
        self.save_size_limit(&file_size_limit).await;
        Ok(())
    }

    async fn try_load_size_limit_from_server(&self) -> Result<FileSize, Error> {
        let request = ruma::api::client::authenticated_media::get_media_config::v1::Request::new();
        let response = self.send(request).await.map_err(Error::from)?;
        Ok(FileSize(u64::from(response.upload_size)))
    }

    async fn save_size_limit(&self, size_limit: &FileSize) {
        if let Err(err) = self.state_store().store(&FILE_SIZE_LIMIT, size_limit.0).await {
            warn!("###BWI### could not save value: {}", err);
        };
    }
}
