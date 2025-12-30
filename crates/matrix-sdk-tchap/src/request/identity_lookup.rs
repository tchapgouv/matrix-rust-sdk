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

//! `POST /_matrix/identity/v2/lookup`
//!
//! Looks up the set of Matrix User IDs which have bound the 3PIDs given, if bindings are available.

pub mod v1 {
    //! `/v2/` ([spec])
    //!
    //! [spec]: https://spec.matrix.org/latest/identity-service-api/#post_matrixidentityv2lookup

    use std::collections::BTreeMap;

    use ruma_common::{
        api::{auth_scheme::AccessToken, request, response},
        metadata,
    };

    metadata! {
        method: POST,
        rate_limited: false,
        authentication: AccessToken,
        history: {
            1.0 => "/_matrix/identity/v2/lookup",
        }
    }

    /// Request type for the `identity_lookup` endpoint.
    #[request(error=ruma_common::api::error::MatrixError)]
    pub struct Request {
        /// Email addresses to look up.
        pub addresses: Vec<String>,

        /// Alorithm used to encode the addresses.
        pub algorithm: String,

        /// The pepper associated to the algorithm.
        pub pepper: String,
    }

    /// Response type for the `identity_lookup` endpoint.
    #[response(error=ruma_common::api::error::MatrixError)]
    pub struct Response {
        /// The mappings of addresses to Matrix IDs.
        pub mappings: BTreeMap<String, String>,
    }

    impl Request {
        /// Creates a new `Request`.
        pub fn new(lookup_address: String, pepper: String) -> Self {
            Self {
                addresses: vec![lookup_address],
                algorithm: "sha256".to_string(),
                pepper: pepper,
            }
        }
    }

    impl Response {
        /// Creates a new `Response` with the given mappings.
        pub fn new(mappings: BTreeMap<String, String>) -> Self {
            Self { mappings }
        }
    }
}
