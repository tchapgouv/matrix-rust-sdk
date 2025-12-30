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

//! `GET /_matrix/identity/v2/hash_details`
//!
//! Looks up the set of Matrix User IDs which have bound the 3PIDs given, if bindings are available.

pub mod v1 {
    //! `/v2/` ([spec])
    //!
    //! [spec]: https://spec.matrix.org/latest/identity-service-api/#get_matrixidentityv2hash_details

    use ruma::{
        api::{auth_scheme::AccessToken, client::uiaa::UiaaResponse, request, response},
        metadata,
    };

    metadata! {
        method: GET,
        rate_limited: false,
        authentication: AccessToken,
        history: {
            1.0 => "/_matrix/identity/v2/hash_details",
        }
    }

    /// Request type for the `identity_hash_details` endpoint.
    #[request(error = UiaaResponse)]
    pub struct Request {}

    /// Response type for the `identity_hash_details` endpoint.
    #[response(error = UiaaResponse)]
    pub struct Response {
        /// The algorithms the server supports.
        pub algorithms: Vec<String>,

        /// The pepper the client MUST use in hashing identifiers, and MUST supply to the /lookup endpoint when performing lookups.
        pub lookup_pepper: String,
    }

    impl Request {
        /// Creates an empty `Request`.
        pub fn new() -> Self {
            Self {}
        }
    }

    impl Response {
        /// Creates a new `Response` with the given mappings.
        pub fn new(algorithms: Vec<String>, lookup_pepper: String) -> Self {
            Self { algorithms, lookup_pepper }
        }
    }
}
