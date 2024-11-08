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

use crate::bwi_bindings::PasswordStrength::{Strong, Weak};
use crate::error::ClientError;
use crate::error::ClientError::Generic;
use matrix_sdk_bwi::regulatory::organization::BWIOrganization;

#[uniffi::export(async_runtime = "tokio")]
pub async fn get_imprint_as_url(homeserver_url: &str) -> Result<String, ClientError> {
    let organization =
        BWIOrganization::from_homeserver_url(homeserver_url).await.map_err(|_err| Generic {
            msg: format!(
                "Unable to fetch the imprint from the homeserver with url {url}",
                url = homeserver_url
            ),
        })?;
    Ok(String::from(organization.get_imprint().as_url()))
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn get_data_privacy_as_url(homeserver_url: &str) -> Result<String, ClientError> {
    let organization =
        BWIOrganization::from_homeserver_url(homeserver_url).await.map_err(|_err| Generic {
            msg: format!(
                "Unable to fetch the privacy policy from the homeserver with url {url}",
                url = homeserver_url
            ),
        })?;
    Ok(String::from(organization.get_data_privacy().as_url()))
}

#[derive(Clone, uniffi::Enum)]
pub enum PasswordStrength {
    Weak,
    Medium,
    Strong,
}

pub fn get_password_strength(password: &str) -> PasswordStrength {
    if password.is_empty() {
        Weak
    } else {
        Strong
    }
}
