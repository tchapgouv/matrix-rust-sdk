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
use matrix_sdk_base::ruma::events::room::MediaSource::Encrypted;
use matrix_sdk_base::ruma::events::room::{
    EncryptedFile, EncryptedFileInit, JsonWebKey, JsonWebKeyInit,
};
use matrix_sdk_base::ruma::exports::serde_json::json;
use matrix_sdk_base::ruma::mxc_uri;
use matrix_sdk_base::ruma::serde::Base64;
use matrix_sdk_base_bwi::content_scanner::scan_state::BWIScanState;
use matrix_sdk_bwi::content_scanner::BWIContentScanner;
use simple_logger::SimpleLogger;
use std::collections::BTreeMap;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

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

fn dummy_jwt() -> JsonWebKey {
    JsonWebKeyInit {
        kty: "oct".to_owned(),
        key_ops: vec!["encrypt".to_owned(), "decrypt".to_owned()],
        alg: "A256CTR".to_owned(),
        k: Base64::new("qcHVMSgYg-71CauWBezXI5qkaRb0LuIy-Wx5kIaHMIA".as_bytes().to_vec()),
        ext: true,
    }
    .into()
}

fn encrypted_file() -> EncryptedFile {
    EncryptedFileInit {
        url: mxc_uri!("mxc://localhost/encryptedfile").to_owned(),
        key: dummy_jwt(),
        iv: Base64::new("X85+XgHN+HEAAAAAAAAAAA".as_bytes().to_vec()),
        hashes: BTreeMap::new(),
        v: "v2".to_owned(),
    }
    .into()
}

#[tokio::test]
async fn test_media_scan_trusted() {
    SimpleLogger::new().env().init().unwrap();
    // Arrange
    let mock_server = wiremock::MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("_matrix/media_proxy/unstable/public_key"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json! ({"public_key": "GdwYYj5Ey9O96FMi4DjIhPhY604RuZg2Om98Kqh+3GE"}),
            ),
        )
        .mount(&mock_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/_matrix/media_proxy/unstable/scan_encrypted"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json! (
            {
                "clean": true,
                "info": "File is clean"
            }
        )))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::builder().build().unwrap();
    let mock_server_url = url::Url::parse(&mock_server.uri()).unwrap();
    let content_scanner = BWIContentScanner::new_with_url(client, mock_server_url);

    // Act
    let scan_state = content_scanner
        .scan_attachment_with_content_scanner(Encrypted(Box::from(encrypted_file())))
        .await;

    // Assert
    assert_eq!(scan_state, BWIScanState::Trusted);
}
