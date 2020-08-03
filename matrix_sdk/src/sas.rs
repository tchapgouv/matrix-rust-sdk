// Copyright 2020 The Matrix.org Foundation C.I.C.
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

use std::sync::Arc;

use url::Url;

use matrix_sdk_base::{Device, Sas as BaseSas, Session};
use matrix_sdk_common::locks::RwLock;

use crate::{error::Result, http_client::HttpClient};

#[allow(dead_code)]
#[derive(Debug, Clone)]
/// An object controling the interactive verification flow.
pub struct Sas {
    pub(crate) inner: BaseSas,
    pub(crate) homeserver: Arc<Url>,
    pub(crate) http_client: HttpClient,
    pub(crate) session: Arc<RwLock<Option<Session>>>,
}

impl Sas {
    /// Accept the interactive verification flow.
    pub async fn accept(&self) -> Result<()> {
        if let Some(request) = self.inner.accept() {
            self.http_client.send(request, self.session.clone()).await?;
        }
        Ok(())
    }

    /// Confirm that the short auth strings match on both sides.
    pub async fn confirm(&self) -> Result<()> {
        if let Some(request) = self.inner.confirm().await? {
            self.http_client.send(request, self.session.clone()).await?;
        }

        Ok(())
    }

    /// Cancel the interactive verification flow.
    pub async fn cancel(&self) -> Result<()> {
        if let Some(request) = self.inner.cancel() {
            self.http_client.send(request, self.session.clone()).await?;
        }
        Ok(())
    }

    /// Get the emoji version of the short auth string.
    pub fn emoji(&self) -> Option<Vec<(&'static str, &'static str)>> {
        self.inner.emoji()
    }

    /// Get the decimal version of the short auth string.
    pub fn decimals(&self) -> Option<(u16, u16, u16)> {
        self.inner.decimals()
    }

    /// Is the verification process done.
    pub fn is_done(&self) -> bool {
        self.inner.is_done()
    }

    /// Is the verification process canceled.
    pub fn is_canceled(&self) -> bool {
        self.inner.is_canceled()
    }

    /// Get the other users device that we're veryfying.
    pub fn other_device(&self) -> Device {
        self.inner.other_device()
    }
}
