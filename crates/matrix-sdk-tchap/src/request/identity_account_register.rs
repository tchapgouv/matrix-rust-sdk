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

//! `POST /_matrix/identity/v2/account/register`
//!
//! Exchanges an OpenID token from the homeserver for an access token to access the identity server.

pub mod v1 {
    //! `/v2/` ([spec])
    //!
    //! [spec]: https://spec.matrix.org/latest/identity-service-api/#post_matrixidentityv2accountregister

    use ruma::{
        api::client::account::request_openid_token::v3::Response as OpenIdResponse,
        authentication::TokenType, OwnedServerName,
    };
    use std::time::Duration;

    use ruma_common::{
        api::{auth_scheme::AccessToken, request, response},
        metadata,
    };

    metadata! {
        method: POST,
        rate_limited: false,
        authentication: AccessToken,
        history: {
            1.0 => "/_matrix/identity/v2/account/register",
        }
    }

    /// Request type for the `identity_account_register` endpoint.
    #[request(error=ruma_common::api::error::MatrixError)]
    pub struct Request {
        /// Access token for verifying user's identity.
        pub access_token: String,

        /// Access token type.
        pub token_type: TokenType,

        /// Homeserver domain for verification of user's identity.
        pub matrix_server_name: OwnedServerName,

        /// Seconds until token expiration.
        #[serde(with = "ruma_common::serde::duration::secs")]
        pub expires_in: Duration,
    }

    /// Response type for the `identity_account_register` endpoint.
    #[response(error=ruma_common::api::error::MatrixError)]
    pub struct Response {
        /// The token to authenticate future requests to the identity server with
        pub token: String,
    }

    impl Request {
        /// Creates a new `Request` from an `OpenIdResponse`.
        pub fn new(data: OpenIdResponse) -> Self {
            Self {
                access_token: data.access_token,
                token_type: data.token_type,
                matrix_server_name: data.matrix_server_name,
                expires_in: data.expires_in,
            }
        }
    }

    impl Response {
        /// Creates a new `Response` with the given token.
        pub fn new(token: String) -> Self {
            Self { token }
        }
    }
}
