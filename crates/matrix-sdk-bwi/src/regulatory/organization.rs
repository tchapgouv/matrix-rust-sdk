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
use crate::regulatory::organization::url_helper::BWIUrlHelper;
use crate::regulatory::well_known_file::BWIWellKnownFileSource;
use anyhow::Result;

pub struct BWIOrganization {
    data_privacy_source: Box<dyn BWIDataPrivacySource>,
    imprint_source: Box<dyn BWIImprintSource>,
}

impl BWIOrganization {
    pub fn new(
        privacy_policy_source: Box<dyn BWIDataPrivacySource>,
        imprint_source: Box<dyn BWIImprintSource>,
    ) -> Self {
        BWIOrganization { data_privacy_source: privacy_policy_source, imprint_source }
    }

    pub async fn from_homeserver_url(homeserver_url_as_str: &str) -> Result<Self> {
        let homeserver_url =
            BWIUrlHelper::with_base_url(homeserver_url_as_str)?.for_well_known_file().get_url();

        let well_known_source = BWIWellKnownFileSource::new(homeserver_url).await?;

        let imprint_source = Box::from(well_known_source.clone());
        let privacy_policy_source = Box::from(well_known_source);

        Ok(BWIOrganization::new(privacy_policy_source, imprint_source))
    }

    pub fn get_imprint(&self) -> BWIImprint {
        self.imprint_source.get_imprint()
    }

    pub fn get_data_privacy(&self) -> BWIDataPrivacy {
        self.data_privacy_source.get_data_privacy()
    }
}

mod url_helper {
    use url::{ParseError, Url};

    const WELL_KNOWN_PATH: &str = ".well-known/matrix/client";

    pub struct BWIUrlHelper {
        url: Url,
    }

    impl BWIUrlHelper {
        fn with_base_url_without_schema(base_url: &str) -> Result<Self, ParseError> {
            let formated_url = format!("https://{url}", url = base_url);
            let parsed_url = Url::parse(formated_url.as_str())?;
            let builder = BWIUrlHelper { url: parsed_url };
            Ok(builder)
        }

        pub fn with_base_url(base_url: &str) -> Result<Self, ParseError> {
            match Url::parse(base_url) {
                Ok(url) => Ok(BWIUrlHelper { url }),
                Err(e) => match e {
                    ParseError::RelativeUrlWithoutBase => {
                        BWIUrlHelper::with_base_url_without_schema(base_url)
                    }
                    _ => Err(e),
                },
            }
        }

        pub fn for_well_known_file(&mut self) -> &Self {
            self.url = self
                .url
                .join(WELL_KNOWN_PATH)
                .expect("The location of the well known file is well known and is valid");
            self
        }

        pub fn get_url(&self) -> Url {
            self.url.clone()
        }
    }

    #[cfg(test)]
    mod url_test {
        use crate::regulatory::organization::url_helper::BWIUrlHelper;
        use url::{ParseError, Url};

        #[test]
        fn only_base_url() -> Result<(), ParseError> {
            let valid_url = "example.com";
            let parsed_url = Url::parse("https://example.com/.well-known/matrix/client")?;

            // Act
            let built_url =
                BWIUrlHelper::with_base_url(valid_url).unwrap().for_well_known_file().get_url();

            // Assert
            Ok(assert_eq!(built_url, parsed_url))
        }

        #[test]
        fn base_url_and_schema() -> Result<(), ParseError> {
            let valid_url = "https://example.com";
            let parsed_url = Url::parse("https://example.com/.well-known/matrix/client")?;

            // Act
            let built_url =
                BWIUrlHelper::with_base_url(valid_url).unwrap().for_well_known_file().get_url();

            // Assert
            Ok(assert_eq!(built_url, parsed_url))
        }
    }
}

#[cfg(test)]
mod test {
    use crate::regulatory::data_privacy::{BWIDataPrivacy, BWIDataPrivacySource};
    use crate::regulatory::imprint::{BWIImprint, BWIImprintSource};
    use crate::regulatory::organization::BWIOrganization;

    struct WellKnownMock {}

    impl BWIImprintSource for WellKnownMock {
        fn get_imprint(&self) -> BWIImprint {
            BWIImprint::new("https://www.bwi.de/impressum")
        }
    }

    impl BWIDataPrivacySource for WellKnownMock {
        fn get_data_privacy(&self) -> BWIDataPrivacy {
            BWIDataPrivacy::new("https://messenger.bwi.de/datenschutz-bundesmessenger")
        }
    }

    #[test]
    fn test_legal_information_imprint_for_organization() {
        // Arrange
        let organization =
            BWIOrganization::new(Box::new(WellKnownMock {}), Box::new(WellKnownMock {}));

        // Act
        let imprint = organization.get_imprint();

        // Assert
        assert_eq!(imprint.as_url(), "https://www.bwi.de/impressum");
    }

    #[test]
    fn test_legal_information_dat_privacy_for_organization() {
        // Arrange
        let organization =
            BWIOrganization::new(Box::new(WellKnownMock {}), Box::new(WellKnownMock {}));

        // Act
        let privacy_policy = organization.get_data_privacy();

        // Assert
        assert_eq!(privacy_policy.as_url(), "https://messenger.bwi.de/datenschutz-bundesmessenger");
    }
}
