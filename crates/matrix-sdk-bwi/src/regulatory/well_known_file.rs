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

use crate::regulatory::data_privacy::{BWIDataPrivacy, BWIDataPrivacySource};
use crate::regulatory::imprint::{BWIImprint, BWIImprintSource};
use anyhow::Result;
use matrix_sdk_base_bwi::http_client::HttpClient;
use reqwest::Url;
use serde::Deserialize;

pub(crate) trait BWIWellKnownFileSource {
    fn get_wellknown_file(&self) -> Result<BWIWellKnownFile>;
}

#[derive(Clone)]
pub(crate) struct BWIWellKnownFileSourceImpl {
    well_known_file: BWIWellKnownFile,
}

#[derive(Deserialize, Clone)]
pub struct BWIWellKnownFileExtension {
    #[serde(rename = "data_privacy_url")]
    pub privacy_policy_url: String,
    #[serde(rename = "imprint_url")]
    pub imprint_url: String,
}

#[derive(Deserialize, Clone)]
pub struct BWIWellKnownFile {
    #[serde(rename = "de.bwi")]
    pub bwi_extension: BWIWellKnownFileExtension,
}

impl BWIWellKnownFile {
    async fn get_from_homeserver(
        homeserver_url: Url,
        http_client: Box<dyn HttpClient>,
    ) -> Result<BWIWellKnownFile> {
        let res = http_client.get(homeserver_url).await?.json::<BWIWellKnownFile>().await?;

        Ok(res)
    }
}

impl BWIWellKnownFileSourceImpl {
    pub async fn new(
        homeserver_url: Url,
        http_client: Box<dyn HttpClient>,
    ) -> Result<BWIWellKnownFileSourceImpl> {
        let well_known_file =
            BWIWellKnownFile::get_from_homeserver(homeserver_url, http_client).await?;

        Ok(BWIWellKnownFileSourceImpl { well_known_file })
    }
}

impl BWIWellKnownFileSource for BWIWellKnownFileSourceImpl {
    fn get_wellknown_file(&self) -> Result<BWIWellKnownFile> {
        Ok(self.well_known_file.clone())
    }
}

impl BWIImprintSource for BWIWellKnownFileSourceImpl {
    fn get_imprint(&self) -> BWIImprint {
        BWIImprint::new(&self.get_wellknown_file().unwrap().bwi_extension.imprint_url)
    }
}

impl BWIDataPrivacySource for BWIWellKnownFileSourceImpl {
    fn get_data_privacy(&self) -> BWIDataPrivacy {
        BWIDataPrivacy::new(&self.get_wellknown_file().unwrap().bwi_extension.privacy_policy_url)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use matrix_sdk_base_bwi::http_client::{HttpClient, HttpError};
    use url::Url;

    const EXAMPLE_WELL_KNOWN: &str = "{
            \"m.homeserver\": {
                \"base_url\": \"https://example.com\"
            },
                \"org.matrix.msc3575.proxy\": {
                \"url\": \"https://bwmdev-sync.example.com:443\"
            },
            \"org.matrix.msc3814\": false,
            \"m.tile_server\": {
                \"map_style_url\": \"https://example.com/style.json\"
            },
            \"io.element.rendezvous\": {
                \"server\": \"https://example.com/_synapse/rendezvous\"
            },
            \"io.element.e2ee\": {
                \"secure_backup_required\": true,
                \"secure_backup_setup_methods\": [
                    \"passphrase\"
                ],
                \"outbound_keys_pre_sharing_mode\": \"on_room_opening\"
            },
            \"de.bwi\": {
                \"data_privacy_url\": \"https://messenger.bwi.de/datenschutz-bundesmessenger\",
                \"imprint_url\": \"https://www.bwi.de/impressum\",
                \"federation\": {
                    \"show_introduction\": true,
                    \"show_announcement\": true,
                    \"enable\": true
                }
            }
        }";

    struct MockHttpClient {}

    #[async_trait]
    impl HttpClient for MockHttpClient {
        async fn get(&self, url: Url) -> Result<reqwest::Response, HttpError> {
            match url.as_str() {
                "https://www.bwi.de/.well-known/matrix/client" => {
                    Ok(reqwest::Response::from(::http::Response::new(EXAMPLE_WELL_KNOWN)))
                }
                _ => Err(HttpError::NotFound),
            }
        }
    }

    #[tokio::test]
    async fn parse_fetched_well_known_file() {
        use crate::regulatory::well_known_file::BWIWellKnownFile;

        // Arrange
        // todo don't require full well known path for fetching
        let homeserver_url = Url::parse("https://www.bwi.de/.well-known/matrix/client").unwrap();
        let mock_client: Box<dyn HttpClient> = Box::from(MockHttpClient {});

        // Act
        let well_known = BWIWellKnownFile::get_from_homeserver(homeserver_url, mock_client)
            .await
            .expect("Could not parse well-known");

        // Assert
        assert_eq!(well_known.bwi_extension.imprint_url, "https://www.bwi.de/impressum");
        assert_eq!(
            well_known.bwi_extension.privacy_policy_url,
            "https://messenger.bwi.de/datenschutz-bundesmessenger"
        );
    }
}
