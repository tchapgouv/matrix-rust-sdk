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

//! `GET /_matrix/media_proxy/unstable/public_key`
//!
//! Retrieve content from the media store.

pub mod v3 {
    use ruma_common::api::error::FromHttpResponseError;
    use ruma_common::api::EndpointError;
    use ruma_common::{
        api::{request, response, Metadata},
        metadata,
    };
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    const METADATA: Metadata = metadata! {
        method: POST,
        rate_limited: false,
        authentication: None,
        history: {
            1.0 => "/_matrix/media_proxy/unstable/scan_encrypted",
            1.1 => "/_matrix/media_proxy/unstable/scan_encrypted",
        }
    };

    #[derive(Serialize, Clone, Debug)]
    pub struct EncryptedFileKey {
        #[serde(default)]
        pub alg: Option<String>,
        #[serde(default)]
        pub ext: Option<bool>,
        #[serde(rename = "keyOps")]
        pub key_ops: Option<Vec<String>>,
        #[serde(default)]
        pub kty: Option<String>,
        #[serde(default)]
        pub k: Option<String>,
    }

    #[derive(Serialize, Clone, Debug)]
    pub struct EncryptedFileInfo {
        #[serde(default)]
        pub url: Option<String>,
        #[serde(default)]
        pub mimetype: Option<String>,
        #[serde(default)]
        pub key: Option<EncryptedFileKey>,
        #[serde(default)]
        pub iv: Option<String>,
        #[serde(default)]
        pub hashes: Option<BTreeMap<String, String>>,
        #[serde(default)]
        pub v: Option<String>,
    }

    #[derive(Serialize, Clone, Debug)]
    pub struct EncryptedBody {
        #[serde(default)]
        pub ciphertext: String,
        #[serde(default)]
        pub mac: String,
        #[serde(default)]
        pub ephemeral: String,
    }

    /// Request type for the `public_key` endpoint.
    #[request(error = ruma_client_api::Error)]
    #[derive(Serialize)]
    pub struct Request {
        #[serde(default)]
        pub file: Option<EncryptedFileInfo>,
        #[serde(default)]
        pub encrypted_body: Option<EncryptedBody>,
    }

    /// Response type for the `public_key` endpoint.
    #[response(error = ruma_client_api::Error)]
    #[derive(Deserialize)]
    pub struct Response {
        #[serde(default)]
        pub clean: bool,
        #[serde(default)]
        pub info: Option<String>,
    }

    impl Request {
        pub fn new(ciphertext: &str, mac: &str, ephemeral: &str) -> Self {
            Self {
                file: None,
                encrypted_body: Some(EncryptedBody {
                    ciphertext: String::from(ciphertext),
                    mac: String::from(mac),
                    ephemeral: String::from(ephemeral),
                }),
            }
        }
    }

    impl Response {
        /// Creates a new `Response` with the given public key
        pub fn new(clean: bool, info: Option<String>) -> Self {
            Self { clean, info }
        }
    }

    impl ruma_common::api::OutgoingRequest for Request {
        type EndpointError = ruma_common::api::error::MatrixError;
        type IncomingResponse = Response;

        const METADATA: Metadata = METADATA;

        fn try_into_http_request<T: Default + bytes::BufMut>(
            self,
            base_url: &str,
            _: ruma_common::api::SendAccessToken<'_>,
            considering_versions: &'_ [ruma_common::api::MatrixVersion],
        ) -> Result<http::Request<T>, ruma_common::api::error::IntoHttpError> {
            use http::header;

            http::Request::builder()
                .method(METADATA.method)
                .uri(METADATA.make_endpoint_url(considering_versions, base_url, &[], "")?)
                .header(header::CONTENT_TYPE, "application/json")
                .body(ruma_common::serde::json_to_buf(&self)?)
                .map_err(Into::into)
        }
    }

    impl ruma_common::api::IncomingResponse for Response {
        type EndpointError = ruma_common::api::error::MatrixError;

        fn try_from_http_response<T: AsRef<[u8]>>(
            response: http::Response<T>,
        ) -> Result<Self, FromHttpResponseError<Self::EndpointError>> {
            if response.status().as_u16() >= 400 {
                return Err(FromHttpResponseError::Server(
                    ruma_common::api::error::MatrixError::from_http_response(response),
                ));
            }
            let body = response.body().as_ref();
            let json = std::str::from_utf8(body)?;
            let res: Response = serde_json::from_str(&json)?;
            Ok(res)
        }
    }

    #[cfg(test)]
    mod test {
        use bytes::BytesMut;
        use ruma_common::api::error::DeserializationError::*;
        use ruma_common::api::{IncomingResponse, MatrixVersion, OutgoingRequest, SendAccessToken};

        use crate::bwi_content_scanner_api::scan_file;

        use super::*;

        #[test]
        fn create_request() {
            let request = scan_file::v3::Request::new("ciphertext", "mac", "ephemeral");
            let homeserver = "https://wwww.example.com";
            let result = request.try_into_http_request::<BytesMut>(
                homeserver,
                SendAccessToken::None,
                &[MatrixVersion::V1_0],
            );
            assert!(result.is_ok());
            let req = result.unwrap();
            let body = req.body().as_ref();
            let json = std::str::from_utf8(body).unwrap();
            assert_eq!(
                "https://wwww.example.com/_matrix/media_proxy/unstable/scan_encrypted",
                req.uri().to_string()
            );
        }

        #[test]
        fn response_valid_json() {
            let response =
                http::Response::builder().status(200).body("{ \"clean\":true}".as_bytes()).unwrap();

            let result = scan_file::v3::Response::try_from_http_response(response);
            assert!(result.is_ok());
            let res = result.unwrap();
            assert_eq!(res.clean, true);
            assert_eq!(res.info, None);
        }
    }
}
