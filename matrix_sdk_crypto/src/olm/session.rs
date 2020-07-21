// Copyright 2020 The Matrix.org Foundation C.I.C.
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

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use olm_rs::errors::OlmSessionError;
use olm_rs::session::OlmSession;
use olm_rs::PicklingMode;

use serde_json::{json, Value};

pub use olm_rs::{
    session::{OlmMessage, PreKeyMessage},
    utility::OlmUtility,
};

use super::Account;
use crate::error::{EventError, OlmResult};
use crate::Device;

use matrix_sdk_common::{
    api::r0::keys::KeyAlgorithm,
    events::{
        room::encrypted::{CiphertextInfo, EncryptedEventContent, OlmV1Curve25519AesSha2Content},
        EventType,
    },
    instant::Instant,
    locks::Mutex,
};

/// Cryptographic session that enables secure communication between two
/// `Account`s
#[derive(Clone)]
pub struct Session {
    pub(crate) inner: Arc<Mutex<OlmSession>>,
    pub(crate) session_id: Arc<String>,
    pub(crate) sender_key: Arc<String>,
    pub(crate) creation_time: Arc<Instant>,
    pub(crate) last_use_time: Arc<Instant>,
}

// #[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("session_id", &self.session_id())
            .field("sender_key", &self.sender_key)
            .finish()
    }
}

impl Session {
    /// Decrypt the given Olm message.
    ///
    /// Returns the decrypted plaintext or an `OlmSessionError` if decryption
    /// failed.
    ///
    /// # Arguments
    ///
    /// * `message` - The Olm message that should be decrypted.
    pub async fn decrypt(&mut self, message: OlmMessage) -> Result<String, OlmSessionError> {
        let plaintext = self.inner.lock().await.decrypt(message)?;
        self.last_use_time = Arc::new(Instant::now());
        Ok(plaintext)
    }

    /// Encrypt the given plaintext as a OlmMessage.
    ///
    /// Returns the encrypted Olm message.
    ///
    /// # Arguments
    ///
    /// * `plaintext` - The plaintext that should be encrypted.
    pub(crate) async fn encrypt_helper(&mut self, plaintext: &str) -> OlmMessage {
        let message = self.inner.lock().await.encrypt(plaintext);
        self.last_use_time = Arc::new(Instant::now());
        message
    }

    /// Encrypt the given event content content as an m.room.encrypted event
    /// content.
    pub async fn encrypt(
        &mut self,
        account: Account,
        recipient_device: &Device,
        event_type: EventType,
        content: Value,
    ) -> OlmResult<EncryptedEventContent> {
        let recipient_signing_key = recipient_device
            .get_key(KeyAlgorithm::Ed25519)
            .ok_or(EventError::MissingSigningKey)?;
        let recipient_sender_key = recipient_device
            .get_key(KeyAlgorithm::Curve25519)
            .ok_or(EventError::MissingSigningKey)?;

        let payload = json!({
            "sender": account.user_id.to_string(),
            "sender_device": account.device_id.as_ref(),
            "keys": {
                "ed25519": account.identity_keys().ed25519(),
            },
            "recipient": recipient_device.user_id(),
            "recipient_keys": {
                "ed25519": recipient_signing_key,
            },
            "type": event_type,
            "content": content,
        });

        let plaintext = cjson::to_string(&payload)
            .unwrap_or_else(|_| panic!(format!("Can't serialize {} to canonical JSON", payload)));

        let ciphertext = self.encrypt_helper(&plaintext).await.to_tuple();

        let message_type = ciphertext.0;
        let ciphertext = CiphertextInfo::new(ciphertext.1, (message_type as u32).into());

        let mut content = BTreeMap::new();
        content.insert(recipient_sender_key.to_owned(), ciphertext);

        Ok(EncryptedEventContent::OlmV1Curve25519AesSha2(
            OlmV1Curve25519AesSha2Content::new(
                content,
                account.identity_keys().curve25519().to_owned(),
            ),
        ))
    }

    /// Check if a pre-key Olm message was encrypted for this session.
    ///
    /// Returns true if it matches, false if not and a OlmSessionError if there
    /// was an error checking if it matches.
    ///
    /// # Arguments
    ///
    /// * `their_identity_key` - The identity/curve25519 key of the account
    /// that encrypted this Olm message.
    ///
    /// * `message` - The pre-key Olm message that should be checked.
    pub async fn matches(
        &self,
        their_identity_key: &str,
        message: PreKeyMessage,
    ) -> Result<bool, OlmSessionError> {
        self.inner
            .lock()
            .await
            .matches_inbound_session_from(their_identity_key, message)
    }

    /// Returns the unique identifier for this session.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Store the session as a base64 encoded string.
    ///
    /// # Arguments
    ///
    /// * `pickle_mode` - The mode that was used to pickle the session, either
    /// an unencrypted mode or an encrypted using passphrase.
    pub async fn pickle(&self, pickle_mode: PicklingMode) -> String {
        self.inner.lock().await.pickle(pickle_mode)
    }

    /// Restore a Session from a previously pickled string.
    ///
    /// Returns the restored Olm Session or a `OlmSessionError` if there was an
    /// error.
    ///
    /// # Arguments
    ///
    /// * `pickle` - The pickled string of the session.
    ///
    /// * `pickle_mode` - The mode that was used to pickle the session, either
    /// an unencrypted mode or an encrypted using passphrase.
    ///
    /// * `sender_key` - The public curve25519 key of the account that
    /// established the session with us.
    ///
    /// * `creation_time` - The timestamp that marks when the session was
    /// created.
    ///
    /// * `last_use_time` - The timestamp that marks when the session was
    /// last used to encrypt or decrypt an Olm message.
    pub fn from_pickle(
        pickle: String,
        pickle_mode: PicklingMode,
        sender_key: String,
        creation_time: Instant,
        last_use_time: Instant,
    ) -> Result<Self, OlmSessionError> {
        let session = OlmSession::unpickle(pickle, pickle_mode)?;
        let session_id = session.session_id();

        Ok(Session {
            inner: Arc::new(Mutex::new(session)),
            session_id: Arc::new(session_id),
            sender_key: Arc::new(sender_key),
            creation_time: Arc::new(creation_time),
            last_use_time: Arc::new(last_use_time),
        })
    }
}

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        self.session_id() == other.session_id()
    }
}
