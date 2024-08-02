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
    use serde::Deserialize;

    const METADATA: Metadata = metadata! {
        method: GET,
        rate_limited: false,
        authentication: None,
        history: {
            1.0 => "/_matrix/media_proxy/unstable/public_key",
            1.1 => "/_matrix/media_proxy/unstable/public_key",
        }
    };

    /// Request type for the `public_key` endpoint.
    #[request(error = ruma_client_api::Error)]
    pub struct Request {}

    /// Response type for the `public_key` endpoint.
    #[response(error = ruma_client_api::Error)]
    #[derive(Deserialize)]
    pub struct Response {
        #[serde(default)]
        pub public_key: String,
    }

    impl Request {
        pub fn new() -> Self {
            Self {}
        }
    }

    impl Response {
        /// Creates a new `Response` with the given public key
        pub fn new(public_key: String) -> Self {
            Self { public_key }
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
                .body(T::default())
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
        use super::*;
        use crate::bwi_content_scanner_api::get_public_key;
        use bytes::BytesMut;
        use ruma_common::api::error::DeserializationError::*;
        use ruma_common::api::error::FromHttpResponseError::{Deserialization, Server};
        use ruma_common::api::{IncomingResponse, MatrixVersion, OutgoingRequest, SendAccessToken};

        #[test]
        fn create_request() {
            let request = get_public_key::v3::Request::new();
            let homeserver = "https://wwww.example.com";
            let result = request.try_into_http_request::<BytesMut>(
                homeserver,
                SendAccessToken::None,
                &[MatrixVersion::V1_0],
            );
            assert!(result.is_ok());
            assert_eq!(
                "https://wwww.example.com/_matrix/media_proxy/unstable/public_key",
                result.unwrap().uri().to_string()
            );
        }

        #[test]
        fn response_with_wrong_http_status() {
            let response =
                http::Response::builder().status(440).body(&[0x41u8, 0x41u8, 0x42u8]).unwrap();

            let result = get_public_key::v3::Response::try_from_http_response(response);
            assert!(result.is_err());
            if let Err(Server(error)) = result {
                assert_eq!(error.status_code, 440);
            } else {
                assert!(false);
            }
        }

        #[test]
        fn response_no_utf8() {
            let response =
                http::Response::builder().status(200).body(&[1, 2, 3, 4, 5, 255, 255]).unwrap();

            let result = get_public_key::v3::Response::try_from_http_response(response);
            assert!(result.is_err());
            if let Err(Deserialization(Utf8(error))) = result {
                assert!(true);
            } else {
                assert!(false);
            }
        }

        #[test]
        fn response_json_error() {
            let response =
                http::Response::builder().status(200).body("hello world".as_bytes()).unwrap();

            let result = get_public_key::v3::Response::try_from_http_response(response);
            assert!(result.is_err());
            if let Err(Deserialization(Json(error))) = result {
                assert!(true);
            } else {
                assert!(false);
            }
        }

        #[test]
        fn response_valid_json() {
            let response = http::Response::builder()
                .status(200)
                .body("{ \"public_key\":\"test\"}".as_bytes())
                .unwrap();

            let result = get_public_key::v3::Response::try_from_http_response(response);
            assert!(result.is_ok());
            let res = result.unwrap();
            assert_eq!(res.public_key, "test");
        }
    }
}
