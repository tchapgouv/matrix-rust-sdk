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
use crate::helpers::{
    clean_state_response_body, encrypted_file, infected_state_response_body, mount_public_key_mock,
    mount_scan_mock,
};
use matrix_sdk_base::ruma::events::room::MediaSource::Encrypted;
use matrix_sdk_base_bwi::content_scanner::scan_state::BWIScanState;
use matrix_sdk_bwi::content_scanner::BWIContentScanner;
use serde_json::json;
use wiremock::ResponseTemplate;

#[tokio::test]
#[ignore]
async fn test_get_public_key() {
    // Arrange
    let client = reqwest::Client::default();
    let content_scanner =
        BWIContentScanner::new_with_url_as_str(client, "example.com").unwrap();

    // Act
    let public_key = content_scanner.get_public_key().await;

    // Assert
    assert!(public_key.is_ok());
    assert_eq!(public_key.unwrap().0, "6eUf9Oa7j6oEvwHcqElti4qu+t36k7gwdwsA3H+xVAI".to_owned());
}

mod helpers {
    use matrix_sdk_base::ruma::events::room::{
        EncryptedFile, EncryptedFileInit, JsonWebKey, JsonWebKeyInit,
    };
    use matrix_sdk_base::ruma::mxc_uri;
    use matrix_sdk_base::ruma::serde::Base64;
    use serde_json::json;
    use std::collections::BTreeMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    pub fn dummy_jwt() -> JsonWebKey {
        JsonWebKeyInit {
            kty: "oct".to_owned(),
            key_ops: vec!["encrypt".to_owned(), "decrypt".to_owned()],
            alg: "A256CTR".to_owned(),
            k: Base64::new("qcHVMSgYg-71CauWBezXI5qkaRb0LuIy-Wx5kIaHMIA".as_bytes().to_vec()),
            ext: true,
        }
        .into()
    }

    pub fn encrypted_file() -> EncryptedFile {
        EncryptedFileInit {
            url: mxc_uri!("mxc://localhost/encryptedfile").to_owned(),
            key: dummy_jwt(),
            iv: Base64::new("X85+XgHN+HEAAAAAAAAAAA".as_bytes().to_vec()),
            hashes: BTreeMap::new(),
            v: "v2".to_owned(),
        }
        .into()
    }

    pub async fn mount_public_key_mock(mock_server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("_matrix/media_proxy/unstable/public_key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json! ({"public_key": "GdwYYj5Ey9O96FMi4DjIhPhY604RuZg2Om98Kqh+3GE"}),
            ))
            .mount(mock_server)
            .await;
    }

    pub async fn mount_scan_mock(mock_server: &MockServer, response: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path("/_matrix/media_proxy/unstable/scan_encrypted"))
            .respond_with(response)
            .mount(mock_server)
            .await;
    }

    pub fn clean_state_response_body() -> serde_json::value::Value {
        json! (
            {
                "clean": true,
                "info": "File is clean"
            }
        )
    }

    pub fn infected_state_response_body() -> serde_json::value::Value {
        json! (
            {
                "clean": false,
                "info": "***VIRUS DETECTED***"
            }
        )
    }
}

#[tokio::test]
async fn test_media_scan_trusted() {
    // Arrange
    let mock_server = wiremock::MockServer::builder().start().await;
    mount_public_key_mock(&mock_server).await;
    mount_scan_mock(
        &mock_server,
        ResponseTemplate::new(200).set_body_json(clean_state_response_body()),
    )
    .await;

    let client = reqwest::Client::builder().build().unwrap();
    let mock_server_url = url::Url::parse(&mock_server.uri()).unwrap();
    let content_scanner = BWIContentScanner::new_with_url(client, mock_server_url);

    // Act
    let scan_state = content_scanner.scan_attachment(Encrypted(Box::from(encrypted_file()))).await;

    // Assert
    assert_eq!(scan_state, BWIScanState::Trusted);
}

#[tokio::test]
async fn test_media_scan_infected_with_200() {
    // Arrange
    let mock_server = wiremock::MockServer::builder().start().await;
    mount_public_key_mock(&mock_server).await;
    mount_scan_mock(
        &mock_server,
        ResponseTemplate::new(200).set_body_json(infected_state_response_body()),
    )
    .await;

    let client = reqwest::Client::builder().build().unwrap();
    let mock_server_url = url::Url::parse(&mock_server.uri()).unwrap();
    let content_scanner = BWIContentScanner::new_with_url(client, mock_server_url);

    // Act
    let scan_state = content_scanner
        .scan_attachment_with_content_scanner(Encrypted(Box::from(encrypted_file())))
        .await;

    // Assert
    assert_eq!(scan_state, BWIScanState::Infected);
}

#[tokio::test]
async fn test_media_scan_infected_with_403_and_not_clean_body() {
    // Arrange
    let mock_server = wiremock::MockServer::builder().start().await;
    mount_public_key_mock(&mock_server).await;
    mount_scan_mock(
        &mock_server,
        ResponseTemplate::new(403).set_body_json(infected_state_response_body()),
    )
    .await;

    let client = reqwest::Client::builder().build().unwrap();
    let mock_server_url = url::Url::parse(&mock_server.uri()).unwrap();
    let content_scanner = BWIContentScanner::new_with_url(client, mock_server_url);

    // Act
    let scan_state = content_scanner
        .scan_attachment_with_content_scanner(Encrypted(Box::from(encrypted_file())))
        .await;

    // Assert
    assert_eq!(scan_state, BWIScanState::Infected);
}

#[tokio::test]
async fn test_media_scan_error_with_403_and_clean_body() {
    // Arrange
    let mock_server = wiremock::MockServer::builder().start().await;
    mount_public_key_mock(&mock_server).await;
    mount_scan_mock(
        &mock_server,
        ResponseTemplate::new(403).set_body_json(clean_state_response_body()),
    )
    .await;

    let client = reqwest::Client::builder().build().unwrap();
    let mock_server_url = url::Url::parse(&mock_server.uri()).unwrap();
    let content_scanner = BWIContentScanner::new_with_url(client, mock_server_url);

    // Act
    let scan_state = content_scanner
        .scan_attachment_with_content_scanner(Encrypted(Box::from(encrypted_file())))
        .await;

    // Assert
    assert_eq!(scan_state, BWIScanState::Error);
}

#[tokio::test]
async fn test_media_scan_error_with_403_and_mime_type_forbidden() {
    // Arrange
    let mock_server = wiremock::MockServer::builder().start().await;
    mount_public_key_mock(&mock_server).await;
    mount_scan_mock(
        &mock_server,
        ResponseTemplate::new(403).set_body_json(json!({"reason": "MCS_MIME_TYPE_FORBIDDEN"})),
    )
    .await;

    let client = reqwest::Client::builder().build().unwrap();
    let mock_server_url = url::Url::parse(&mock_server.uri()).unwrap();
    let content_scanner = BWIContentScanner::new_with_url(client, mock_server_url);

    // Act
    let scan_state = content_scanner
        .scan_attachment_with_content_scanner(Encrypted(Box::from(encrypted_file())))
        .await;

    // Assert
    assert_eq!(scan_state, BWIScanState::Error);
}
