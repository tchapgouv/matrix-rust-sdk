// Copyright 2023 The Matrix.org Foundation C.I.C.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;

use base64::{
    alphabet,
    engine::{general_purpose, GeneralPurpose},
    Engine,
};
use gloo_utils::format::JsValueSerdeExt;
use matrix_sdk_crypto::CryptoStoreError;
use matrix_sdk_store_encryption::{EncryptedValueBase64, StoreCipher};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use wasm_bindgen::JsValue;
use web_sys::IdbKeyRange;

use crate::{safe_encode::SafeEncode, IndexeddbCryptoStoreError};

type Result<A, E = IndexeddbCryptoStoreError> = std::result::Result<A, E>;

const BASE64: GeneralPurpose = GeneralPurpose::new(&alphabet::STANDARD, general_purpose::NO_PAD);

/// Handles the functionality of serializing and encrypting data for the
/// indexeddb store.
pub struct IndexeddbSerializer {
    store_cipher: Option<Arc<StoreCipher>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MaybeEncrypted {
    Encrypted(EncryptedValueBase64),
    Unencrypted(String),
}

impl IndexeddbSerializer {
    pub fn new(store_cipher: Option<Arc<StoreCipher>>) -> Self {
        Self { store_cipher }
    }

    /// Hash the given key securely for the given tablename, using the store
    /// cipher.
    ///
    /// First calls [`SafeEncode::as_encoded_string`]
    /// on the `key` to encode it into a formatted string.
    ///
    /// Then, if a cipher is configured, hashes the formatted key and returns
    /// the hash encoded as unpadded base64.
    ///
    /// If no cipher is configured, just returns the formatted key.
    ///
    /// This is faster than [`Self::serialize_value`] and reliably gives the
    /// same output for the same input, making it suitable for index keys.
    pub fn encode_key<T>(&self, table_name: &str, key: T) -> JsValue
    where
        T: SafeEncode,
    {
        self.encode_key_as_string(table_name, key).into()
    }

    /// Hash the given key securely for the given tablename, using the store
    /// cipher.
    ///
    /// The same as [`Self::encode_key`], but stops short of converting the
    /// resulting base64 string into a JsValue
    pub fn encode_key_as_string<T>(&self, table_name: &str, key: T) -> String
    where
        T: SafeEncode,
    {
        match &self.store_cipher {
            Some(cipher) => key.as_secure_string(table_name, cipher),
            None => key.as_encoded_string(),
        }
    }

    pub fn encode_to_range<T>(
        &self,
        table_name: &str,
        key: T,
    ) -> Result<IdbKeyRange, IndexeddbCryptoStoreError>
    where
        T: SafeEncode,
    {
        match &self.store_cipher {
            Some(cipher) => key.encode_to_range_secure(table_name, cipher),
            None => key.encode_to_range(),
        }
        .map_err(|e| IndexeddbCryptoStoreError::DomException {
            code: 0,
            name: "IdbKeyRangeMakeError".to_owned(),
            message: e,
        })
    }

    /// Encode the value for storage as a value in indexeddb.
    ///
    /// First, serialise the given value as JSON.
    ///
    /// Then, if a store cipher is enabled, encrypt the JSON string using the
    /// configured store cipher, giving a byte array. Then, wrap the byte
    /// array as a `JsValue`.
    ///
    /// If no cipher is enabled, deserialises the JSON string again giving a JS
    /// object.
    pub fn serialize_value(&self, value: &impl Serialize) -> Result<JsValue, CryptoStoreError> {
        if let Some(cipher) = &self.store_cipher {
            let value = cipher.encrypt_value(value).map_err(CryptoStoreError::backend)?;

            // Turn the Vec<u8> into a Javascript-side `Array<number>`.
            // XXX Isn't there a way to do this that *doesn't* involve going via a JSON
            // string?
            Ok(JsValue::from_serde(&value)?)
        } else {
            // Turn the rust-side struct into a JS-side `Object`.
            Ok(JsValue::from_serde(&value)?)
        }
    }

    /// Encode the value for storage as a value in indexeddb.
    ///
    /// This is the same algorithm as [`Self::serialize_value`], but stops short
    /// of encoding the resultant byte vector in a JsValue.
    ///
    /// Returns a byte vector which is either the JSON serialisation of the
    /// value, or an encrypted version thereof.
    pub fn serialize_value_as_bytes(
        &self,
        value: &impl Serialize,
    ) -> Result<Vec<u8>, CryptoStoreError> {
        match &self.store_cipher {
            Some(cipher) => cipher.encrypt_value(value).map_err(CryptoStoreError::backend),
            None => serde_json::to_vec(value).map_err(CryptoStoreError::backend),
        }
    }

    /// Encode an object for storage as a value in indexeddb.
    pub fn maybe_encrypt_value<T: Serialize>(
        &self,
        value: T,
    ) -> Result<MaybeEncrypted, CryptoStoreError> {
        Ok(match &self.store_cipher {
            Some(cipher) => MaybeEncrypted::Encrypted(
                cipher.encrypt_value_base64_typed(&value).map_err(CryptoStoreError::backend)?,
            ),
            None => MaybeEncrypted::Unencrypted(
                BASE64.encode(serde_json::to_vec(&value).map_err(CryptoStoreError::backend)?),
            ),
        })
    }

    /// Decode a value that was previously encoded with
    /// [`Self::serialize_value`]
    pub fn deserialize_value<T: DeserializeOwned>(
        &self,
        value: JsValue,
    ) -> Result<T, CryptoStoreError> {
        if let Some(cipher) = &self.store_cipher {
            // `value` is a JS-side array containing the byte values. Turn it into a
            // rust-side Vec<u8>.
            // XXX: Isn't there a way to do this that *doesn't* involve going via a JSON
            // string?
            let value: Vec<u8> = value.into_serde()?;

            cipher.decrypt_value(&value).map_err(CryptoStoreError::backend)
        } else {
            Ok(value.into_serde()?)
        }
    }

    /// Decode a value that was previously encoded with
    /// [`Self::serialize_value_as_bytes`]
    pub fn deserialize_value_from_bytes<T: DeserializeOwned>(
        &self,
        value: &[u8],
    ) -> Result<T, CryptoStoreError> {
        if let Some(cipher) = &self.store_cipher {
            cipher.decrypt_value(value).map_err(CryptoStoreError::backend)
        } else {
            serde_json::from_slice(value).map_err(CryptoStoreError::backend)
        }
    }

    /// Decode a value that was previously encoded with
    /// [`Self::maybe_encrypt_value`]
    pub fn maybe_decrypt_value<T: DeserializeOwned>(
        &self,
        value: MaybeEncrypted,
    ) -> Result<T, CryptoStoreError> {
        match (&self.store_cipher, value) {
            (Some(cipher), MaybeEncrypted::Encrypted(enc)) => {
                cipher.decrypt_value_base64_typed(enc).map_err(CryptoStoreError::backend)
            }
            (None, MaybeEncrypted::Unencrypted(unc)) => {
                Ok(serde_json::from_slice(&BASE64.decode(unc).map_err(CryptoStoreError::backend)?)
                    .map_err(CryptoStoreError::backend)?)
            }

            _ => Err(CryptoStoreError::UnpicklingError),
        }
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use std::sync::Arc;

    use matrix_sdk_store_encryption::StoreCipher;
    use matrix_sdk_test::async_test;
    use serde::{Deserialize, Serialize};

    use super::IndexeddbSerializer;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    /// Test that `serialize_value`/`deserialize_value` will round-trip, when a
    /// cipher is in use.
    #[async_test]
    async fn test_serialize_deserialize_with_cipher() {
        let serializer = IndexeddbSerializer::new(Some(Arc::new(StoreCipher::new().unwrap())));

        let obj = make_test_object();
        let serialized = serializer.serialize_value(&obj).expect("could not serialize");
        let deserialized: TestStruct =
            serializer.deserialize_value(serialized).expect("could not deserialize");

        assert_eq!(obj, deserialized);
    }

    /// Test that `serialize_value`/`deserialize_value` will round-trip, when no
    /// cipher is in use.
    #[async_test]
    async fn test_serialize_deserialize_no_cipher() {
        let serializer = IndexeddbSerializer::new(None);
        let obj = make_test_object();
        let serialized = serializer.serialize_value(&obj).expect("could not serialize");
        let deserialized: TestStruct =
            serializer.deserialize_value(serialized).expect("could not deserialize");

        assert_eq!(obj, deserialized);
    }

    /// Test that `maybe_encrypt_value`/`maybe_decrypt_value` will round-trip,
    /// when a cipher is in use.
    #[async_test]
    async fn test_maybe_encrypt_decrypt_with_cipher() {
        let serializer = IndexeddbSerializer::new(Some(Arc::new(StoreCipher::new().unwrap())));

        let obj = make_test_object();
        let serialized = serializer.maybe_encrypt_value(&obj).expect("could not serialize");
        let deserialized: TestStruct =
            serializer.maybe_decrypt_value(serialized).expect("could not deserialize");

        assert_eq!(obj, deserialized);
    }

    /// Test that `maybe_encrypt_value`/`maybe_decrypt_value` will round-trip,
    /// when no cipher is in use.
    #[async_test]
    async fn test_maybe_encrypt_decrypt_no_cipher() {
        let serializer = IndexeddbSerializer::new(None);

        let obj = make_test_object();
        let serialized = serializer.maybe_encrypt_value(&obj).expect("could not serialize");
        let deserialized: TestStruct =
            serializer.maybe_decrypt_value(serialized).expect("could not deserialize");

        assert_eq!(obj, deserialized);
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestStruct {
        id: u32,
        name: String,
    }

    fn make_test_object() -> TestStruct {
        TestStruct { id: 0, name: "test".to_owned() }
    }
}
