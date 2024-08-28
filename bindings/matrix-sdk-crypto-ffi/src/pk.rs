use std::sync::Arc;

use matrix_sdk_crypto::backups::{
    Message as InnerMessage, PkDecryption as InnerPkDecryption, PkEncryption as InnerPkEncryption,
};
use vodozemac::Curve25519PublicKey;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum PkError {
    #[error("Unexpected key length {0}")]
    KeyLength(usize),
    #[error(transparent)]
    Decrypt(#[from] matrix_sdk_crypto::backups::DecryptionError),
}

#[derive(uniffi::Record)]
pub struct Message {
    ciphertext: Vec<u8>,
    mac: Vec<u8>,
    ephemeral_key: Vec<u8>,
}

/// A decryption object.
#[derive(uniffi::Object)]
pub struct PkDecryption {
    pub(crate) inner: InnerPkDecryption,
}

#[uniffi::export]
impl PkDecryption {
    // Initialize a decryption object by creating a new secret key on the way.
    #[allow(clippy::new_without_default)]
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self { inner: InnerPkDecryption::new() })
    }

    #[uniffi::constructor]
    pub fn from_key(bytes: &[u8]) -> Result<Arc<Self>, PkError> {
        let length = bytes.len();

        if length == 32 {
            let mut key = Box::new([0u8; 32]);
            key.copy_from_slice(bytes);

            Ok(Arc::new(Self { inner: InnerPkDecryption::from_bytes(&key) }))
        } else {
            Err(PkError::KeyLength(length))
        }
    }

    // The secret key used to decrypt messages.
    pub fn key(&self) -> Vec<u8> {
        self.inner.key_bytes().to_vec()
    }

    // The public key used to encrypt messages for this decryption object.
    pub fn public_key(&self) -> Vec<u8> {
        self.inner.public_key().to_vec()
    }

    /// Decrypt a ciphertext. See the PkEncryption::encrypt function
    /// for descriptions of the ephemeral_key and mac arguments.
    pub fn decrypt(&self, message: Message) -> Result<Vec<u8>, PkError> {
        let ephemeral_key_length = message.ephemeral_key.len();

        if ephemeral_key_length == 32 {
            let mut ephemeral_key_bytes = [0u8; 32];
            ephemeral_key_bytes.copy_from_slice(message.ephemeral_key.as_slice());

            let message = InnerMessage {
                ciphertext: message.ciphertext,
                mac: message.mac,
                ephemeral_key: Curve25519PublicKey::from_bytes(ephemeral_key_bytes),
            };

            self.inner.decrypt(&message).map_err(|e| PkError::Decrypt(e))
        } else {
            Err(PkError::KeyLength(ephemeral_key_length))
        }
    }
}

/// An encryption object.
#[derive(uniffi::Object)]
pub struct PkEncryption {
    pub(crate) inner: InnerPkEncryption,
}

#[uniffi::export]
impl PkEncryption {
    // Initialize an encryption object with the public key of the recipient.
    // The public key must be 32 bytes.
    #[uniffi::constructor]
    pub fn from_public_key(bytes: &[u8]) -> Result<Arc<Self>, PkError> {
        let length = bytes.len();

        if length == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(bytes);

            Ok(Arc::new(Self {
                inner: InnerPkEncryption::from_key(Curve25519PublicKey::from_bytes(key)),
            }))
        } else {
            Err(PkError::KeyLength(length))
        }
    }

    /// Encrypt a plaintext for the recipient. Writes to the ciphertext, mac, and
    /// ephemeral_key buffers, whose values should be sent to the recipient. mac is
    /// a Message Authentication Code to ensure that the data is received and
    /// decrypted properly. ephemeral_key is the public part of the ephemeral key
    /// used (together with the recipient's key) to generate a symmetric encryption
    /// key.
    pub fn encrypt(&self, message: &[u8]) -> Message {
        let msg = self.inner.encrypt(message);
        Message {
            ciphertext: msg.ciphertext.to_vec(),
            mac: msg.mac.to_vec(),
            ephemeral_key: msg.ephemeral_key.to_vec(),
        }
    }
}
