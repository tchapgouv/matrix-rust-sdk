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
use matrix_sdk_base::crypto::vodozemac::pk_encryption::{Message, PkEncryption};
use matrix_sdk_base::crypto::vodozemac::Curve25519PublicKey;
use matrix_sdk_base::ruma::events::room::EncryptedFile;
use matrix_sdk_base::ruma::exports::serde_json;
use matrix_sdk_base::ruma::serde::base64::Standard;
use matrix_sdk_base::ruma::serde::Base64;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct BWIContentScannerPublicKey(pub String);

impl From<BWIPublicKeyDto> for BWIContentScannerPublicKey {
    fn from(public_key: BWIPublicKeyDto) -> Self {
        Self(public_key.public_key)
    }
}

impl TryFrom<&BWIContentScannerPublicKey> for Curve25519PublicKey {
    type Error = anyhow::Error;

    fn try_from(value: &BWIContentScannerPublicKey) -> anyhow::Result<Self> {
        Self::from_base64(&value.0).map_err(|e| anyhow::anyhow!(e))
    }
}

#[derive(Deserialize)]
pub struct BWIPublicKeyDto {
    pub public_key: String,
}

#[derive(Deserialize)]
pub struct BWIScanStateResultDto {
    pub clean: bool,
    #[allow(dead_code)]
    pub info: String,
}

#[derive(Serialize)]
pub struct BWIEncryptedFileDto {
    file: EncryptedFile,
}

impl BWIEncryptedFileDto {
    pub fn new(file: EncryptedFile) -> Self {
        Self { file }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct EncryptedMetadataRequest {
    pub encrypted_body: EncryptedMetadata,
}

impl EncryptedMetadataRequest {
    pub fn new(encrypted_body: EncryptedMetadata) -> Self {
        Self { encrypted_body }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct EncryptedMetadata {
    pub ciphertext: Base64,
    pub mac: Base64,
    pub ephemeral: Base64,
}

fn encode_as_base_64(bytes: Vec<u8>) -> Base64 {
    Base64::<Standard>::new(bytes)
}

impl From<Message> for EncryptedMetadata {
    fn from(message: Message) -> Self {
        Self {
            ciphertext: encode_as_base_64(message.ciphertext),
            mac: encode_as_base_64(message.mac),
            ephemeral: encode_as_base_64(message.ephemeral_key.to_vec()),
        }
    }
}

pub struct EncryptedMetadataRequestBuilder(BWIEncryptedFileDto);

impl EncryptedMetadataRequestBuilder {
    pub fn for_encrypted_file(file: EncryptedFile) -> Self {
        Self(BWIEncryptedFileDto::new(file))
    }

    pub fn build_encrypted_request(
        self,
        key: &BWIContentScannerPublicKey,
    ) -> anyhow::Result<EncryptedMetadataRequest> {
        let encryption = PkEncryption::from_key(Curve25519PublicKey::try_from(key)?);
        let message = serde_json::to_string(&self.0).map_err(|e| anyhow::anyhow!(e))?;
        let encrypted_metadata = encryption.encrypt(message.as_bytes());
        Ok(EncryptedMetadataRequest::new(EncryptedMetadata::from(encrypted_metadata)))
    }
}

#[cfg(test)]
mod test_encrypted_file {
    use crate::content_scanner::dto::BWIEncryptedFileDto;
    use matrix_sdk_base::ruma::events::room::{EncryptedFile, EncryptedFileInit, JsonWebKeyInit};
    use matrix_sdk_base::ruma::exports::serde_json;
    use matrix_sdk_base::ruma::exports::serde_json::json;
    use matrix_sdk_base::ruma::mxc_uri;
    use matrix_sdk_base::ruma::serde::Base64;
    use std::collections::BTreeMap;

    #[test]
    fn test_serialization_suitable_for_content_scanner_request() {
        // Arrange
        let expected_serialized_file_body = json!({
            "file":{
                "v": "v2",
                "key": {
                    "alg": "A256CTR",
                    "ext": true,
                    "k": "qcHVMSgYg-71CauWBezXI5qkaRb0LuIy-Wx5kIaHMIA",
                    "key_ops": [
                        "encrypt",
                        "decrypt"
                    ],
                    "kty": "oct"
                },
                "iv": "X85+XgHN+HEAAAAAAAAAAA",
                "hashes": {
                    "sha256": "5qG4fFnbbVdlAB1Q72JDKwCagV6Dbkx9uds4rSak37c"
                },
                "url": "mxc://matrix.org/oSTbuSlyZKXvgtbtUsPxRbto"
            }
        });

        let encrypted_file: EncryptedFile = EncryptedFileInit {
            url: mxc_uri!("mxc://matrix.org/oSTbuSlyZKXvgtbtUsPxRbto").to_owned(),
            key: JsonWebKeyInit {
                kty: "oct".to_owned(),
                key_ops: vec!["encrypt".to_owned(), "decrypt".to_owned()],
                alg: "A256CTR".to_owned(),
                k: Base64::parse(b"qcHVMSgYg-71CauWBezXI5qkaRb0LuIy-Wx5kIaHMIA").unwrap(),
                ext: true,
            }
            .into(),
            iv: Base64::parse(b"X85+XgHN+HEAAAAAAAAAAA").unwrap(),
            hashes: BTreeMap::from([(
                String::from("sha256"),
                Base64::parse(b"5qG4fFnbbVdlAB1Q72JDKwCagV6Dbkx9uds4rSak37c").unwrap(),
            )]),
            v: "v2".to_owned(),
        }
        .into();

        // Act
        let encrypted_file_wrapper = BWIEncryptedFileDto::new(encrypted_file);
        let serialized_request_body = serde_json::to_value(&encrypted_file_wrapper);

        // Assert
        assert!(serialized_request_body.is_ok());
        assert_eq!(expected_serialized_file_body, serialized_request_body.unwrap());
    }
}
