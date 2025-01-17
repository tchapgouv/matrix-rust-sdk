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

use matrix_sdk_base_bwi::jwt_token::BWIJWTTokenValidationError::NoValidPublicKey;
use matrix_sdk_base_bwi::jwt_token::{BWIPublicKeyForJWTTokenValidation, BWITokenValidator};
use url::Url;

const VALID_PUB_KEYFILE: &str = "tests/resources/valid_jwt_key.key.pub";
const INVALID_PUB_KEYFILE: &str = "tests/resources/invalid_jwt_key.key.pub";
const INVALID_PEM_KEYFILE: &str = "tests/resources/invalid_jwt_key.pem";

#[tokio::test]
#[ignore]
async fn test_valid_jwt_token_from_pub_file() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange
    let key = BWIPublicKeyForJWTTokenValidation::from_file(VALID_PUB_KEYFILE).unwrap();

    // Act
    let homeserver_url = Url::parse("https://example.com").unwrap();

    // Assert
    assert_eq!(
        Ok(()),
        BWITokenValidator::for_homeserver(homeserver_url).validate_with_keys(&vec![key]).await
    );
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_invalid_jwt_token_from_pem_file() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange
    let key = BWIPublicKeyForJWTTokenValidation::from_file(INVALID_PEM_KEYFILE).unwrap();

    // Act
    let homeserver_url = Url::parse("https://example.com").unwrap();

    // Assert
    assert_eq!(
        Err(NoValidPublicKey()),
        BWITokenValidator::for_homeserver(homeserver_url).validate_with_keys(&[key]).await
    );
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_valid_jwt_token_from_multiple_pub_file() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange
    let invalid_key = BWIPublicKeyForJWTTokenValidation::from_file(INVALID_PUB_KEYFILE).unwrap();
    let valid_key = BWIPublicKeyForJWTTokenValidation::from_file(VALID_PUB_KEYFILE).unwrap();

    // Act
    let homeserver_url = Url::parse("https://example.com").unwrap();

    // Assert
    assert_eq!(
        Ok(()),
        BWITokenValidator::for_homeserver(homeserver_url)
            .validate_with_keys(&[invalid_key, valid_key])
            .await
    );
    Ok(())
}
