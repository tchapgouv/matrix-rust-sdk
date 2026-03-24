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
use crate::content_scanner::url::content_scanner_api::Endpoint::PublicKeyEndpoint;
use url::Url;

pub(crate) mod content_scanner_api {
    use crate::content_scanner::url::BWIContentScannerUrl;
    use url::Url;

    pub enum Endpoint {
        PublicKeyEndpoint,
    }

    impl From<Endpoint> for String {
        fn from(value: Endpoint) -> Self {
            match value {
                Endpoint::PublicKeyEndpoint => {
                    "/_matrix/media_proxy/unstable/public_key".to_owned()
                }
            }
        }
    }

    impl BWIContentScannerUrl {
        pub(crate) fn get_url_for_endpoint(&self, endpoint: Endpoint) -> Url {
            let endpoint_as_str: String = endpoint.into();
            self.get_base_url().join(&endpoint_as_str).expect("The url should be valid")
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BWIContentScannerUrl {
    base_url: Url,
}

impl BWIContentScannerUrl {
    fn new(base_url: Url) -> Self {
        Self { base_url }
    }
    pub fn for_base_url_as_string(base_url: &str) -> Result<Self, url::ParseError> {
        let fully_qualified_url = Self::sanitize_base_url(base_url);
        let base_url = Url::parse(&fully_qualified_url)?;
        Ok(Self::for_base_url(base_url))
    }

    pub fn for_base_url(base_url: Url) -> Self {
        Self::new(base_url)
    }

    fn sanitize_base_url(base_url: &str) -> String {
        if base_url.starts_with("https://") {
            String::from(base_url)
        } else {
            format!("https://{}/", base_url)
        }
    }

    pub(crate) fn get_base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn get_public_key_url(&self) -> Url {
        self.get_url_for_endpoint(PublicKeyEndpoint)
    }
}

#[cfg(test)]
mod test {
    use crate::content_scanner::url::BWIContentScannerUrl;

    #[test]
    fn test_valid_base_url() {
        const BASE_URL: &str = "example.com";
        const EXPECTED_BASE_URL: &str = "https://example.com/";

        // Act
        let content_scanner_url = BWIContentScannerUrl::for_base_url_as_string(BASE_URL);

        // Assert
        assert!(content_scanner_url.is_ok());
        assert_eq!(EXPECTED_BASE_URL, content_scanner_url.unwrap().get_base_url().as_str());
    }

    #[test]
    fn test_valid_base_url_with_protocol() {
        const BASE_URL: &str = "https://example.com";
        const EXPECTED_BASE_URL: &str = "https://example.com/";

        // Act
        let content_scanner_url = BWIContentScannerUrl::for_base_url_as_string(BASE_URL);

        // Assert
        assert!(content_scanner_url.is_ok());
        assert_eq!(EXPECTED_BASE_URL, content_scanner_url.unwrap().get_base_url().as_str());
    }

    #[test]
    fn test_valid_base_url_get_public_key_url() {
        const BASE_URL: &str = "https://example.com";
        const EXPECTED_PUBLIC_KEY_URL: &str =
            "https://example.com/_matrix/media_proxy/unstable/public_key";

        // Act
        let content_scanner_url = BWIContentScannerUrl::for_base_url_as_string(BASE_URL).unwrap();

        // Assert
        assert_eq!(EXPECTED_PUBLIC_KEY_URL, content_scanner_url.get_public_key_url().as_str());
    }
}
