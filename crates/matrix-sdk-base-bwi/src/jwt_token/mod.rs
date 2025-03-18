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

use crate::jwt_token::BWIPublicKeyForJWTTokenValidationParseError::PublicKeyLoadingFailed;
use jsonwebtoken::errors::ErrorKind::InvalidSignature;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::{fs, io};
use url::Url;

const JWT_TOKEN_FOR_VALIDATION_ENDPOINT: &str = "_bum/client/v1/verify";

#[derive(Debug, Serialize, Deserialize)]
pub struct BWIJwtToken {
    sub: String,
    exp: f64,
}

#[derive(Clone, Debug)]
pub enum BWIPublicKeyForJWTTokenValidationParseError {
    PublicKeyLoadingFailed(io::ErrorKind),
    InvalidPublicKey(),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BWIJWTTokenValidationError {
    TokenFetchFailed(),
    NoValidPublicKey(),
    NoPublicKeysProvided(),
}

#[derive(Clone, Debug)]
pub struct BWIPublicKeyForJWTTokenValidation {
    public_key: Vec<u8>,
}

impl BWIPublicKeyForJWTTokenValidation {
    pub fn from_file(file_path: &str) -> Result<Self, BWIPublicKeyForJWTTokenValidationParseError> {
        let path = Path::new(file_path);
        let public_key =
            fs::read_to_string(path).map_err(|err| PublicKeyLoadingFailed(err.kind()))?;
        BWIPublicKeyForJWTTokenValidation::from_string(&public_key)
    }

    // No impl From<String> as this has a different semantic meaning, aka being a factory-method
    pub fn from_string(
        public_key_as_string: &str,
    ) -> Result<Self, BWIPublicKeyForJWTTokenValidationParseError> {
        Ok(BWIPublicKeyForJWTTokenValidation {
            public_key: public_key_as_string.as_bytes().to_owned(),
        })
    }

    // No impl From<String> as this has a different semantic meaning, aka being a factory-method
    pub fn from_u8(
        public_key_as_string: &[u8],
    ) -> Result<Self, BWIPublicKeyForJWTTokenValidationParseError> {
        Ok(BWIPublicKeyForJWTTokenValidation { public_key: public_key_as_string.to_owned() })
    }

    pub fn as_decoding_key(
        &self,
    ) -> Result<DecodingKey, BWIPublicKeyForJWTTokenValidationParseError> {
        DecodingKey::from_rsa_pem(&self.public_key)
            .map_err(|_| BWIPublicKeyForJWTTokenValidationParseError::InvalidPublicKey())
    }
}

pub struct BWITokenValidator {
    homeserver_url: Url,
}

impl BWITokenValidator {
    pub fn for_homeserver(homeserver_url: Url) -> Self {
        BWITokenValidator { homeserver_url }
    }

    fn jwt_url(&self) -> Url {
        self.homeserver_url
            .join(JWT_TOKEN_FOR_VALIDATION_ENDPOINT)
            .expect("The Url should be valid here")
    }

    fn homeserver_domain(&self) -> String {
        self.homeserver_url.domain().expect("The url of the domain should be valid").to_owned()
    }

    pub async fn validate_with_keys(
        &self,
        keys: &[BWIPublicKeyForJWTTokenValidation],
    ) -> Result<(), BWIJWTTokenValidationError> {
        if keys.is_empty() {
            return Err(BWIJWTTokenValidationError::NoPublicKeysProvided());
        }

        let tokens = self.fetch_jwt_token(self.jwt_url()).await?;

        match self.validate_jwt_tokens_with_keys(&mut tokens.into_iter(), keys) {
            true => Ok(()),
            false => Err(BWIJWTTokenValidationError::NoValidPublicKey()),
        }
    }

    fn validate_jwt_token_with_keys(
        &self,
        token: &str,
        keys: &[BWIPublicKeyForJWTTokenValidation],
    ) -> bool {
        keys.iter().any(|key| self.validate_jwt_token_with_key(token, key))
    }

    fn validate_jwt_tokens_with_keys(
        &self,
        tokens: &mut impl Iterator<Item = String>,
        keys: &[BWIPublicKeyForJWTTokenValidation],
    ) -> bool {
        tokens.any(|token| self.validate_jwt_token_with_keys(&token, keys))
    }

    fn validate_jwt_token_with_key(
        &self,
        encoded_token: &str,
        key: &BWIPublicKeyForJWTTokenValidation,
    ) -> bool {
        let mut validation = Validation::new(Algorithm::PS512);
        validation.sub = Some(self.homeserver_domain());
        validation.set_required_spec_claims(&["exp", "sub"]);
        let decoding_key = key.as_decoding_key().expect("Failed to decode key");
        match decode::<BWIJwtToken>(encoded_token, &decoding_key, &validation) {
            Ok(_token) => true,
            Err(e) => match e.kind() {
                InvalidSignature => {
                    debug!("Token verification failed because of {:?}", e);
                    false
                }
                _ => {
                    error!("Token verification failed because of {:?}", e);
                    false
                }
            },
        }
    }

    async fn fetch_jwt_token(
        &self,
        homeserver_url: Url,
    ) -> Result<Vec<String>, BWIJWTTokenValidationError> {
        let raw_token = reqwest::get(homeserver_url)
            .await
            .map_err(|_| BWIJWTTokenValidationError::TokenFetchFailed())?
            .text()
            .await
            .map_err(|_| BWIJWTTokenValidationError::TokenFetchFailed())?;
        let token_array: Vec<String> = serde_json::from_str(&raw_token)
            .map_err(|_| BWIJWTTokenValidationError::TokenFetchFailed())?;
        debug!("Available Tokens {:?}", token_array);
        Ok(token_array)
    }
}
