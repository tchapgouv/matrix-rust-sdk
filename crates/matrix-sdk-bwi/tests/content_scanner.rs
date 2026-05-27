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
use matrix_sdk_base_bwi::http_client::HttpError::NotFound;
use matrix_sdk_bwi::content_scanner::{BWIContentScanner, BWIContentScannerError};
use matrix_sdk_bwi_test::media::mock_server::content_scanner::{
    provide_public_key_endpoint, EXAMPLE_PUBLIC_KEY,
};
use matrix_sdk_test::async_test;
use wiremock::MockServer;

fn setup_content_scanner(mock_server: &MockServer) -> BWIContentScanner {
    let client = reqwest::Client::builder().build().unwrap();
    let mock_server_url = url::Url::parse(&mock_server.uri()).unwrap();
    BWIContentScanner::new_with_url(&client, &mock_server_url)
}

#[async_test]
async fn test_get_public_key() {
    // Arrange
    let mock_server = MockServer::builder().start().await;

    provide_public_key_endpoint(&mock_server).await;
    let content_scanner = setup_content_scanner(&mock_server);

    // Act
    let public_key = content_scanner.get_public_key().await;

    // Assert
    assert!(public_key.is_ok());
    assert_eq!(public_key.unwrap().0, EXAMPLE_PUBLIC_KEY.to_owned());
}

#[async_test]
async fn test_get_public_key_not_found() {
    // Arrange
    let mock_server = MockServer::builder().start().await;
    let content_scanner = setup_content_scanner(&mock_server);

    // Act
    let public_key = content_scanner.get_public_key().await;

    // Assert
    assert_eq!(public_key.unwrap_err(), BWIContentScannerError::PublicKeyNotAvailable(NotFound));
}
