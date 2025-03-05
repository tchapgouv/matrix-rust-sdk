//! `GET /_matrix/client/*/media/download/{serverName}/{mediaId}`
//!
//! Retrieve content from the media store.

pub mod v1 {
    pub mod download_encrypted {
        //! `/v1/` ([spec])
        //!
        //! [spec]: https://spec.matrix.org/latest/client-server-api/#get_matrixclientv1mediadownloadservernamemediaid

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
            authentication: AccessToken,
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
}
