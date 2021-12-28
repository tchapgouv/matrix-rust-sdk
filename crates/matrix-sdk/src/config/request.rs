// Copyright 2021 The Matrix.org Foundation C.I.C.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    fmt::{self, Debug},
    time::Duration,
};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Configuration for requests the `Client` makes.
///
/// This sets how often and for how long a request should be repeated. As well
/// as how long a successful request is allowed to take.
///
/// By default requests are retried indefinitely and use no timeout.
///
/// # Example
///
/// ```
/// use matrix_sdk::config::RequestConfig;
/// use std::time::Duration;
///
/// // This sets makes requests fail after a single send request and sets the timeout to 30s
/// let request_config = RequestConfig::new()
///     .disable_retry()
///     .timeout(Duration::from_secs(30));
/// ```
#[derive(Copy, Clone)]
pub struct RequestConfig {
    pub(crate) timeout: Duration,
    pub(crate) retry_limit: Option<u64>,
    pub(crate) retry_timeout: Option<Duration>,
    pub(crate) force_auth: bool,
    pub(crate) assert_identity: bool,
}

#[cfg(not(tarpaulin_include))]
impl Debug for RequestConfig {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = fmt.debug_struct("RequestConfig");

        res.field("timeout", &self.timeout)
            .field("retry_limit", &self.retry_limit)
            .field("retry_timeout", &self.retry_timeout)
            .finish()
    }
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_REQUEST_TIMEOUT,
            retry_limit: Default::default(),
            retry_timeout: Default::default(),
            force_auth: false,
            assert_identity: false,
        }
    }
}

impl RequestConfig {
    /// Create a new default `RequestConfig`.
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }

    /// This is a convince method to disable the retries of a request. Setting
    /// the `retry_limit` to `0` has the same effect.
    #[must_use]
    pub fn disable_retry(mut self) -> Self {
        self.retry_limit = Some(0);
        self
    }

    /// The number of times a request should be retried. The default is no limit
    #[must_use]
    pub fn retry_limit(mut self, retry_limit: u64) -> Self {
        self.retry_limit = Some(retry_limit);
        self
    }

    /// Set the timeout duration for all HTTP requests.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set a timeout for how long a request should be retried. The default is
    /// no timeout, meaning requests are retried forever.
    #[must_use]
    pub fn retry_timeout(mut self, retry_timeout: Duration) -> Self {
        self.retry_timeout = Some(retry_timeout);
        self
    }

    /// Force sending authorization even if the endpoint does not require it.
    /// Default is only sending authorization if it is required.
    #[must_use]
    pub fn force_auth(mut self) -> Self {
        self.force_auth = true;
        self
    }

    /// All outgoing http requests will have a GET query key-value appended with
    /// `user_id` being the key and the `user_id` from the `Session` being
    /// the value. Will error if there's no `Session`. This is called
    /// [identity assertion] in the Matrix Application Service Spec
    ///
    /// [identity assertion]: https://spec.matrix.org/unstable/application-service-api/#identity-assertion
    #[cfg(feature = "appservice")]
    #[must_use]
    pub fn assert_identity(mut self) -> Self {
        self.assert_identity = true;
        self
    }
}
