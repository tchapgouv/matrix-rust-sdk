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

pub mod v1 {
    /// Follows the encrypted download endpoint as given in
    /// https://github.com/element-hq/matrix-content-scanner-python/blob/main/docs/api.md
    use crate::content_scanner::dto::EncryptedMetadataRequest;
    #[allow(unused_imports)] // actually used in the macros
    use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
    use ruma_common::{
        api::{request, response, Metadata},
        http_headers::ContentDisposition,
        metadata,
    };

    #[allow(dead_code)]
    const METADATA: Metadata = metadata! {
        method: POST,
        rate_limited: true,
        authentication: AccessTokenOptional,
        history: {
            unstable => "/_matrix/media_proxy/unstable/download_encrypted",
            1.11 => "/_matrix/media_proxy/unstable/download_encrypted",
        }
    };

    /// Request type for the `get_media_content` endpoint.
    #[allow(dead_code)] // needed because of the macro complexity
    #[request(error=ruma_common::api::error::MatrixError)]
    pub struct Request {
        #[ruma_api(body)]
        pub encrypted_metadata: EncryptedMetadataRequest,
    }

    /// Response type for the `get_media_content` endpoint.
    #[allow(dead_code)] // needed because of the macro complexity
    #[response(error=ruma_common::api::error::MatrixError)]
    pub struct Response {
        /// The content that was previously uploaded.
        #[ruma_api(raw_body)]
        pub file: Vec<u8>,

        /// The content type of the file that was previously uploaded.
        #[ruma_api(header = CONTENT_TYPE)]
        pub content_type: Option<String>,

        /// The value of the `Content-Disposition` HTTP header, possibly containing the name of the
        /// file that was previously uploaded.
        #[ruma_api(header = CONTENT_DISPOSITION)]
        pub content_disposition: Option<ContentDisposition>,
    }

    impl Request {
        /// Creates a new `Request` with the encrypted metadata
        pub fn new(encrypted_metadata: EncryptedMetadataRequest) -> Self {
            Self { encrypted_metadata }
        }

        /// Creates a new `Request` with the given URI.
        pub fn from_encrypted_metadata(encrypted_metadata: EncryptedMetadataRequest) -> Self {
            Self::new(encrypted_metadata)
        }
    }
}
