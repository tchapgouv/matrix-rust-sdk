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
pub mod content_scanner {
    use matrix_sdk_base::crypto::MediaEncryptionInfo;
    use matrix_sdk_base::ruma::events::room::{EncryptedFile, EncryptedFileInit};
    use matrix_sdk_base::ruma::mxc_uri;
    use serde_json::json;
    use wiremock::http::Method;
    use wiremock::matchers::{bearer_token, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    pub const EXAMPLE_PUBLIC_KEY: &str = "BwMqVfr1myxqX8tikIPYCyNtpHgMLIg/2nUE+pLQnTE=";
    pub const EXAMPLE_CONTENT: &str = "Hello World!\n";
    pub const DEFAULT_BEARER_TOKEN_FOR_TEST: &str = "1234";

    #[allow(dead_code)]
    const EXAMPLE_PRIVATE_KEY: &str = "KOrUWVTgb7F+1KdpLn+2lRQDeeCuDaMkrhQ5ke6P4HM=";

    pub fn encrypted_file() -> EncryptedFile {
        create_encrypted_file_from_plain_text(EXAMPLE_CONTENT).0
    }

    fn create_encrypted_file_from_plain_text(plain_text: &str) -> (EncryptedFile, Vec<u8>) {
        let (encryption_information, encrypted_as_bin) = encrypt(plain_text);

        let encrypted_file_information = EncryptedFileInit {
            url: mxc_uri!("mxc://localhost/encryptedfile").to_owned(),
            key: encryption_information.key,
            iv: encryption_information.iv,
            hashes: encryption_information.hashes,
            v: encryption_information.version,
        }
        .into();

        (encrypted_file_information, encrypted_as_bin)
    }

    pub fn encrypt(data: &str) -> (MediaEncryptionInfo, Vec<u8>) {
        use matrix_sdk_base::crypto::AttachmentEncryptor;
        use std::io::{Cursor, Read};

        let data = data.to_owned();
        let mut cursor = Cursor::new(data);

        let mut encryptor = AttachmentEncryptor::new(&mut cursor);

        let mut encrypted: Vec<u8> = Vec::new();
        encryptor.read_to_end(&mut encrypted).unwrap();

        (encryptor.finish(), encrypted)
    }

    pub async fn provide_public_key_endpoint(mock_server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("_matrix/media_proxy/unstable/public_key"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"public_key": EXAMPLE_PUBLIC_KEY})),
            )
            .mount(mock_server)
            .await;
    }

    pub async fn provide_supported_versions_endpoint(
        mock_server: &MockServer,
        supported_version: Vec<String>,
    ) {
        Mock::given(method("GET"))
            .and(path("/_matrix/client/versions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "versions": supported_version,
            })))
            .expect(1)
            .mount(mock_server)
            .await;
    }

    /// Provides encrypted download endpoint and expects the default Bearer token
    pub async fn provide_encrypted_download_endpoint(
        mock_server: &MockServer,
        unencrypted_content: &str,
    ) -> EncryptedFile {
        let (encrypted_file_data, encrypted_file_as_byte) =
            create_encrypted_file_from_plain_text(unencrypted_content);
        Mock::given(method(Method::POST))
            .and(path("_matrix/media_proxy/unstable/download_encrypted"))
            .and(bearer_token(DEFAULT_BEARER_TOKEN_FOR_TEST))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(encrypted_file_as_byte, "text/plain"),
            )
            .mount(mock_server)
            .await;
        encrypted_file_data
    }

    pub async fn provide_encrypted_scan_endpoint(
        mock_server: &MockServer,
        unencrypted_content: &str,
        response: ResponseTemplate,
    ) -> EncryptedFile {
        let (encrypted_file_data, _encrypted_file_as_byte) =
            create_encrypted_file_from_plain_text(unencrypted_content);
        Mock::given(method(Method::POST))
            .and(path("_matrix/media_proxy/unstable/scan_encrypted"))
            .and(bearer_token(DEFAULT_BEARER_TOKEN_FOR_TEST))
            .respond_with(response)
            .expect(1)
            .mount(mock_server)
            .await;
        encrypted_file_data
    }

    pub async fn provide_scan_endpoint(mock_server: &MockServer, response: ResponseTemplate) {
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
