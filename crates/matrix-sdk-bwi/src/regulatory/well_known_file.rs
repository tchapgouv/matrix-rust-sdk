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

use crate::regulatory::data_privacy::{BWIDataPrivacy, BWIDataPrivacySource};
use crate::regulatory::imprint::{BWIImprint, BWIImprintSource};
use anyhow::Result;
use reqwest::Url;
use serde::Deserialize;

#[derive(Clone)]
pub(crate) struct BWIWellKnownFileSource {
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
    async fn get_from_homeserver(homeserver_url: Url) -> Result<BWIWellKnownFile> {
        let res = reqwest::get(homeserver_url).await?.json::<BWIWellKnownFile>().await?;

        Ok(res)
    }
}

impl BWIWellKnownFileSource {
    pub async fn new(homeserver_url: Url) -> Result<BWIWellKnownFileSource> {
        let well_known_file = BWIWellKnownFile::get_from_homeserver(homeserver_url).await?;

        Ok(BWIWellKnownFileSource { well_known_file })
    }
}

impl BWIImprintSource for BWIWellKnownFileSource {
    fn get_imprint(&self) -> BWIImprint {
        BWIImprint::new(&self.well_known_file.bwi_extension.imprint_url)
    }
}

impl BWIDataPrivacySource for BWIWellKnownFileSource {
    fn get_data_privacy(&self) -> BWIDataPrivacy {
        BWIDataPrivacy::new(&self.well_known_file.bwi_extension.privacy_policy_url)
    }
}

mod tests {

    #[test]
    fn parse_well_known_file() {
        use crate::regulatory::well_known_file::{BWIWellKnownFile, BWIWellKnownFileExtension};

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

        // Act
        let well_known: BWIWellKnownFile = serde_json::from_str(EXAMPLE_WELL_KNOWN).unwrap();
        let bwi_well_known_extension: BWIWellKnownFileExtension = well_known.bwi_extension;

        // Assert
        assert_eq!(bwi_well_known_extension.imprint_url, "https://www.bwi.de/impressum");
        assert_eq!(
            bwi_well_known_extension.privacy_policy_url,
            "https://messenger.bwi.de/datenschutz-bundesmessenger"
        );
    }
}
