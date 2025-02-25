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

//! extensions for handling attachments

use crate::Client;
use async_trait::async_trait;
use matrix_sdk_bwi::attachment::FILE_SIZE_LIMIT;
use matrix_sdk_bwi::settings_cache::BWISettingsCache;

/// Type for the FileSize
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd)]
pub struct FileSize(pub u64);

impl FileSize {
    /// ctor
    pub fn new(size: u64) -> Self {
        FileSize(size)
    }
}

/// Extension for getting the limit for the file size
#[async_trait]
pub trait ClientAttachmentExt {
    /// get the maximal size for uploading files
    async fn get_size_limit_for_file_upload(&self) -> Option<FileSize>;
}

#[async_trait]
impl ClientAttachmentExt for Client {
    async fn get_size_limit_for_file_upload(&self) -> Option<FileSize> {
        self.try_load_size_limit_from_cache().await
    }
}

#[async_trait]
trait InnerClientAttachmentExt {
    async fn try_load_size_limit_from_cache(&self) -> Option<FileSize>;
}

#[async_trait]
impl InnerClientAttachmentExt for Client {
    async fn try_load_size_limit_from_cache(&self) -> Option<FileSize> {
        self.store().try_load(&FILE_SIZE_LIMIT).await.map(FileSize::new)
    }
}
