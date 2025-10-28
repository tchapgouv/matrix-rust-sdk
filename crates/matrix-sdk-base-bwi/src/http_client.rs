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
use crate::http_client::HttpError::{Failed, Undefined};
use async_trait::async_trait;
use reqwest::{Response, StatusCode};
use std::fmt::{Display, Formatter};
use thiserror::Error;
use url::Url;

#[derive(Error, Debug, Eq, PartialEq)]
pub enum HttpError {
    NotFound,
    Failed(u16),
    Undefined,
}

impl Display for HttpError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::NotFound => write!(f, "not found"),
            Failed(code) => write!(f, "failed with status code {}", code),
            Undefined => write!(f, "undefined"),
        }
    }
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn get(&self, url: Url) -> Result<Response, HttpError>;
}

fn map_reqwest_error(e: reqwest::Error) -> HttpError {
    match e.status() {
        Some(StatusCode::NOT_FOUND) => HttpError::NotFound,
        Some(status) => Failed(status.as_u16()),
        None => Undefined,
    }
}

#[async_trait]
impl HttpClient for reqwest::Client {
    async fn get(&self, url: Url) -> Result<Response, HttpError> {
        self.get(url).send().await.map_err(map_reqwest_error)
    }
}
