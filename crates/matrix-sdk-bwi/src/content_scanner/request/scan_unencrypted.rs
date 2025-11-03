/*
 * MIT License
 *
 * Copyright (c) 2025. DINUM
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
 * DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
 * OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE
 * OR OTHER DEALINGS IN THE SOFTWARE.
 */

pub mod v1 {
    //! `/v1/` ([spec])
    //!
    //! [spec]: https://github.com/element-hq/matrix-content-scanner-python/blob/main/docs/api.md#get-_matrixmedia_proxyunstablescanservernamemediaid

    use crate::content_scanner::request::scan_encrypted::v1::Response;
    use matrix_sdk_base::ruma::OwnedMxcUri;
    use ruma_common::{
        api::{request, Metadata},
        metadata,
    };

    const METADATA: Metadata = metadata! {
        method: GET,
        rate_limited: true,
        authentication: AccessTokenOptional,
        history: {
            1.11 => "/_matrix/media_proxy/unstable/scan/{server_name}/{media_id}",
        }
    };

    /// Request type for the `scan` endpoint.
    #[request(error=ruma_common::api::error::MatrixError)]
    pub struct Request {
        #[ruma_api(path)]
        pub server_name: String,

        #[ruma_api(path)]
        pub media_id: String,
    }

    impl Request {
        /// Creates a new `Request` with the given mxc_uri.
        pub fn new(mxc_uri: &OwnedMxcUri) -> Self {
            Self {
                server_name: mxc_uri.server_name().unwrap().as_str().to_owned(),
                media_id: mxc_uri.media_id().unwrap().to_owned(),
            }
        }
    }
}
