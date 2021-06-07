// Copyright 2021 The Matrix.org Foundation C.I.C.
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

use std::{
    convert::TryFrom,
    io::{Cursor, Read},
};

use byteorder::{BigEndian, ReadBytesExt};
#[cfg(feature = "decode_image")]
use image::{DynamicImage, ImageBuffer, Luma};
use qrcode::QrCode;
use ruma_identifiers::EventId;

#[cfg(feature = "decode_image")]
#[cfg_attr(feature = "docs", doc(cfg(decode_image)))]
use crate::utils::decode_qr;
use crate::{
    error::{DecodingError, EncodingError},
    utils::{base_64_encode, to_bytes, to_qr_code, HEADER, MAX_MODE, MIN_SECRET_LEN, VERSION},
};

/// An enum representing the different modes a QR verification can be in.
#[derive(Clone, Debug, PartialEq)]
pub enum QrVerification {
    /// The QR verification is verifying another user
    Verification(VerificationData),
    /// The QR verification is self-verifying and the current device trusts or
    /// owns the master key
    SelfVerification(SelfVerificationData),
    /// The QR verification is self-verifying in which the current device does
    /// not yet trust the master key
    SelfVerificationNoMasterKey(SelfVerificationNoMasterKey),
}

#[cfg(feature = "decode_image")]
#[cfg_attr(feature = "docs", doc(cfg(decode_image)))]
impl TryFrom<DynamicImage> for QrVerification {
    type Error = DecodingError;

    fn try_from(image: DynamicImage) -> Result<Self, Self::Error> {
        Self::from_image(image)
    }
}

#[cfg(feature = "decode_image")]
#[cfg_attr(feature = "docs", doc(cfg(decode_image)))]
impl TryFrom<ImageBuffer<Luma<u8>, Vec<u8>>> for QrVerification {
    type Error = DecodingError;

    fn try_from(image: ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<Self, Self::Error> {
        Self::from_luma(image)
    }
}

impl TryFrom<&[u8]> for QrVerification {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<Vec<u8>> for QrVerification {
    type Error = DecodingError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl QrVerification {
    /// Decode and parse an image of a QR code into a `QrVerification`
    ///
    /// The image will be converted into a grey scale image before decoding is
    /// attempted
    ///
    /// # Arguments
    ///
    /// * `image` - The image containing the QR code.
    ///
    /// # Example
    /// ```no_run
    /// # use matrix_qrcode::{QrVerification, DecodingError};
    /// # fn main() -> Result<(), DecodingError> {
    /// use image;
    ///
    /// let image = image::open("/path/to/my/image.png").unwrap();
    /// let result = QrVerification::from_image(image)?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "decode_image")]
    #[cfg_attr(feature = "docs", doc(cfg(decode_image)))]
    pub fn from_image(image: DynamicImage) -> Result<Self, DecodingError> {
        let image = image.to_luma8();
        Self::decode(image)
    }

    /// Decode and parse an grey scale image of a QR code into a
    /// `QrVerification`
    ///
    /// # Arguments
    ///
    /// * `image` - The grey scale image containing the QR code.
    ///
    /// # Example
    /// ```no_run
    /// # use matrix_qrcode::{QrVerification, DecodingError};
    /// # fn main() -> Result<(), DecodingError> {
    /// use image;
    ///
    /// let image = image::open("/path/to/my/image.png").unwrap();
    /// let image = image.to_luma8();
    /// let result = QrVerification::from_luma(image)?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "decode_image")]
    #[cfg_attr(feature = "docs", doc(cfg(decode_image)))]
    pub fn from_luma(image: ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<Self, DecodingError> {
        Self::decode(image)
    }

    /// Parse the decoded payload of a QR code in byte slice form as a
    /// `QrVerification`
    ///
    /// This method is useful if you would like to do your own custom QR code
    /// decoding.
    ///
    /// # Arguments
    ///
    /// * `bytes` - The raw bytes of a decoded QR code.
    ///
    /// # Example
    /// ```
    /// # use matrix_qrcode::{QrVerification, DecodingError};
    /// # fn main() -> Result<(), DecodingError> {
    /// let data = b"MATRIX\
    ///              \x02\x02\x00\x07\
    ///              FLOW_ID\
    ///              AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
    ///              BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB\
    ///              SHARED_SECRET";
    ///
    /// let result = QrVerification::from_bytes(data)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, DecodingError> {
        Self::decode_bytes(bytes)
    }

    /// Encode the `QrVerification` into a `QrCode`.
    ///
    /// This method turns the `QrVerification` into a QR code that can be
    /// rendered and presented to be scanned.
    ///
    /// The encoding can fail if the data doesn't fit into a QR code or if the
    /// identity keys that should be encoded into the QR code are not valid
    /// base64.
    ///
    /// # Example
    /// ```
    /// # use matrix_qrcode::{QrVerification, DecodingError};
    /// # fn main() -> Result<(), DecodingError> {
    /// let data = b"MATRIX\
    ///              \x02\x02\x00\x07\
    ///              FLOW_ID\
    ///              AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
    ///              BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB\
    ///              SHARED_SECRET";
    ///
    /// let result = QrVerification::from_bytes(data)?;
    /// let encoded = result.to_qr_code().unwrap();
    /// # Ok(())
    /// # }
    /// ```
    pub fn to_qr_code(&self) -> Result<QrCode, EncodingError> {
        match self {
            QrVerification::Verification(v) => v.to_qr_code(),
            QrVerification::SelfVerification(v) => v.to_qr_code(),
            QrVerification::SelfVerificationNoMasterKey(v) => v.to_qr_code(),
        }
    }

    /// Encode the `QrVerification` into a vector of bytes that can be encoded
    /// as a QR code.
    ///
    /// The encoding can fail if the identity keys that should be encoded are
    /// not valid base64.
    ///
    /// # Example
    /// ```
    /// # use matrix_qrcode::{QrVerification, DecodingError};
    /// # fn main() -> Result<(), DecodingError> {
    /// let data = b"MATRIX\
    ///              \x02\x02\x00\x07\
    ///              FLOW_ID\
    ///              AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
    ///              BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB\
    ///              SHARED_SECRET";
    ///
    /// let result = QrVerification::from_bytes(data)?;
    /// let encoded = result.to_bytes().unwrap();
    ///
    /// assert_eq!(data.as_ref(), encoded.as_slice());
    /// # Ok(())
    /// # }
    /// ```
    pub fn to_bytes(&self) -> Result<Vec<u8>, EncodingError> {
        match self {
            QrVerification::Verification(v) => v.to_bytes(),
            QrVerification::SelfVerification(v) => v.to_bytes(),
            QrVerification::SelfVerificationNoMasterKey(v) => v.to_bytes(),
        }
    }

    /// Decode the byte slice containing the decoded QR code data.
    ///
    /// The format is defined in the [spec].
    ///
    /// The byte slice consists of the following parts:
    ///
    /// * the ASCII string MATRIX
    /// * one byte indicating the QR code version (must be 0x02)
    /// * one byte indicating the QR code verification mode. one of the
    ///   following
    /// values:
    ///     * 0x00 verifying another user with cross-signing
    ///     * 0x01 self-verifying in which the current device does trust the
    ///       master key
    ///     * 0x02 self-verifying in which the current device does not yet trust
    ///       the master key
    /// * the event ID or transaction_id of the associated verification request
    ///   event, encoded as:
    ///     * two bytes in network byte order (big-endian) indicating the length
    ///       in bytes of the ID as a UTF-8 string
    ///     * the ID as a UTF-8 string
    /// * the first key, as 32 bytes
    /// * the second key, as 32 bytes
    /// * a random shared secret, as a byte string. as we do not share the
    ///   length of the secret, and it is not a fixed size, clients will just
    ///   use the remainder of binary string as the shared secret.
    ///
    /// [spec]: https://spec.matrix.org/unstable/client-server-api/#qr-code-format
    fn decode_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, DecodingError> {
        let mut decoded = Cursor::new(bytes);

        let mut header = [0u8; 6];
        let mut first_key = [0u8; 32];
        let mut second_key = [0u8; 32];

        decoded.read_exact(&mut header)?;
        let version = decoded.read_u8()?;
        let mode = decoded.read_u8()?;

        if header != HEADER {
            return Err(DecodingError::Header);
        } else if version != VERSION {
            return Err(DecodingError::Version(version));
        } else if mode > MAX_MODE {
            return Err(DecodingError::Mode(mode));
        }

        let flow_id_len = decoded.read_u16::<BigEndian>()?;
        let mut flow_id = vec![0; flow_id_len.into()];

        decoded.read_exact(&mut flow_id)?;
        decoded.read_exact(&mut first_key)?;
        decoded.read_exact(&mut second_key)?;

        let mut shared_secret = Vec::new();

        decoded.read_to_end(&mut shared_secret)?;

        if shared_secret.len() < MIN_SECRET_LEN {
            return Err(DecodingError::SharedSecret(shared_secret.len()));
        }

        QrVerification::new(mode, flow_id, first_key, second_key, shared_secret)
    }

    /// Decode the given image of an QR code and if we find a valid code, try to
    /// decode it as a `QrVerification`.
    #[cfg(feature = "decode_image")]
    fn decode(image: ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<QrVerification, DecodingError> {
        let decoded = decode_qr(image)?;
        Self::decode_bytes(decoded)
    }

    fn new(
        mode: u8,
        flow_id: Vec<u8>,
        first_key: [u8; 32],
        second_key: [u8; 32],
        shared_secret: Vec<u8>,
    ) -> Result<Self, DecodingError> {
        let first_key = base_64_encode(&first_key);
        let second_key = base_64_encode(&second_key);
        let flow_id = String::from_utf8(flow_id)?;
        let shared_secret = base_64_encode(&shared_secret);

        match mode {
            VerificationData::QR_MODE => {
                let event_id = EventId::try_from(flow_id)?;
                Ok(VerificationData::new(event_id, first_key, second_key, shared_secret).into())
            }
            SelfVerificationData::QR_MODE => {
                Ok(SelfVerificationData::new(flow_id, first_key, second_key, shared_secret).into())
            }
            SelfVerificationNoMasterKey::QR_MODE => {
                Ok(SelfVerificationNoMasterKey::new(flow_id, first_key, second_key, shared_secret)
                    .into())
            }
            m => Err(DecodingError::Mode(m)),
        }
    }

    /// Get the flow id for this `QrVerification`.
    ///
    /// This represents the ID as a string even if it is a `EventId`.
    pub fn flow_id(&self) -> &str {
        match self {
            QrVerification::Verification(v) => v.event_id.as_str(),
            QrVerification::SelfVerification(v) => &v.transaction_id,
            QrVerification::SelfVerificationNoMasterKey(v) => &v.transaction_id,
        }
    }

    /// Get the first key of this `QrVerification`.
    pub fn first_key(&self) -> &str {
        match self {
            QrVerification::Verification(v) => &v.first_master_key,
            QrVerification::SelfVerification(v) => &v.master_key,
            QrVerification::SelfVerificationNoMasterKey(v) => &v.device_key,
        }
    }

    /// Get the second key of this `QrVerification`.
    pub fn second_key(&self) -> &str {
        match self {
            QrVerification::Verification(v) => &v.second_master_key,
            QrVerification::SelfVerification(v) => &v.device_key,
            QrVerification::SelfVerificationNoMasterKey(v) => &v.master_key,
        }
    }

    /// Get the secret of this `QrVerification`.
    pub fn secret(&self) -> &str {
        match self {
            QrVerification::Verification(v) => &v.shared_secret,
            QrVerification::SelfVerification(v) => &v.shared_secret,
            QrVerification::SelfVerificationNoMasterKey(v) => &v.shared_secret,
        }
    }
}

/// The non-encoded data for the first mode of QR code verification.
///
/// This mode is used for verification between two users using their master
/// cross signing keys.
#[derive(Clone, Debug, PartialEq)]
pub struct VerificationData {
    event_id: EventId,
    first_master_key: String,
    second_master_key: String,
    shared_secret: String,
}

impl VerificationData {
    const QR_MODE: u8 = 0x00;

    /// Create a new `VerificationData` struct that can be encoded as a QR code.
    ///
    /// # Arguments
    /// * `event_id` - The event id of the `m.key.verification.request` event
    /// that initiated the verification flow this QR code should be part of.
    ///
    /// * `first_key` - Our own cross signing master key. Needs to be encoded as
    /// unpadded base64
    ///
    /// * `second_key` - The cross signing master key of the other user.
    ///
    /// * ` shared_secret` - A random bytestring encoded as unpadded base64,
    /// needs to be at least 8 bytes long.
    pub fn new(
        event_id: EventId,
        first_key: String,
        second_key: String,
        shared_secret: String,
    ) -> Self {
        Self { event_id, first_master_key: first_key, second_master_key: second_key, shared_secret }
    }

    /// Encode the `VerificationData` into a vector of bytes that can be
    /// encoded as a QR code.
    ///
    /// The encoding can fail if the master keys that should be encoded are not
    /// valid base64.
    ///
    /// # Example
    /// ```
    /// # use matrix_qrcode::{QrVerification, DecodingError};
    /// # fn main() -> Result<(), DecodingError> {
    /// let data = b"MATRIX\
    ///              \x02\x00\x00\x0f\
    ///              $test:localhost\
    ///              AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
    ///              BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB\
    ///              SHARED_SECRET";
    ///
    /// let result = QrVerification::from_bytes(data)?;
    /// if let QrVerification::Verification(decoded) = result {
    ///     let encoded = decoded.to_bytes().unwrap();
    ///     assert_eq!(data.as_ref(), encoded.as_slice());
    /// } else {
    ///     panic!("Data was encoded as an incorrect mode");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn to_bytes(&self) -> Result<Vec<u8>, EncodingError> {
        to_bytes(
            Self::QR_MODE,
            self.event_id.as_str(),
            &self.first_master_key,
            &self.second_master_key,
            &self.shared_secret,
        )
    }

    /// Encode the `VerificationData` into a `QrCode`.
    ///
    /// This method turns the `VerificationData` into a QR code that can be
    /// rendered and presented to be scanned.
    ///
    /// The encoding can fail if the data doesn't fit into a QR code or if the
    /// keys that should be encoded into the QR code are not valid base64.
    pub fn to_qr_code(&self) -> Result<QrCode, EncodingError> {
        to_qr_code(
            Self::QR_MODE,
            self.event_id.as_str(),
            &self.first_master_key,
            &self.second_master_key,
            &self.shared_secret,
        )
    }
}

impl From<VerificationData> for QrVerification {
    fn from(data: VerificationData) -> Self {
        Self::Verification(data)
    }
}

/// The non-encoded data for the second mode of QR code verification.
///
/// This mode is used for verification between two devices of the same user
/// where this device, that is creating this QR code, is trusting or owning
/// the cross signing master key.
#[derive(Clone, Debug, PartialEq)]
pub struct SelfVerificationData {
    transaction_id: String,
    master_key: String,
    device_key: String,
    shared_secret: String,
}

impl SelfVerificationData {
    const QR_MODE: u8 = 0x01;

    /// Create a new `SelfVerificationData` struct that can be encoded as a QR
    /// code.
    ///
    /// # Arguments
    /// * `transaction_id` - The transaction id of this verification flow, the
    /// transaction id was sent by the `m.key.verification.request` event
    /// that initiated the verification flow this QR code should be part of.
    ///
    /// * `master_key` - Our own cross signing master key. Needs to be encoded
    ///   as
    /// unpadded base64
    ///
    /// * `device_key` - The ed25519 key of the other device, encoded as
    /// unpadded base64.
    ///
    /// * ` shared_secret` - A random bytestring encoded as unpadded base64,
    /// needs to be at least 8 bytes long.
    pub fn new(
        transaction_id: String,
        master_key: String,
        device_key: String,
        shared_secret: String,
    ) -> Self {
        Self { transaction_id, master_key, device_key, shared_secret }
    }

    /// Encode the `SelfVerificationData` into a vector of bytes that can be
    /// encoded as a QR code.
    ///
    /// The encoding can fail if the keys that should be encoded are not valid
    /// base64.
    ///
    /// # Example
    /// ```
    /// # use matrix_qrcode::{QrVerification, DecodingError};
    /// # fn main() -> Result<(), DecodingError> {
    /// let data = b"MATRIX\
    ///              \x02\x01\x00\x06\
    ///              FLOWID\
    ///              AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
    ///              BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB\
    ///              SHARED_SECRET";
    ///
    /// let result = QrVerification::from_bytes(data)?;
    /// if let QrVerification::SelfVerification(decoded) = result {
    ///     let encoded = decoded.to_bytes().unwrap();
    ///     assert_eq!(data.as_ref(), encoded.as_slice());
    /// } else {
    ///     panic!("Data was encoded as an incorrect mode");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn to_bytes(&self) -> Result<Vec<u8>, EncodingError> {
        to_bytes(
            Self::QR_MODE,
            &self.transaction_id,
            &self.master_key,
            &self.device_key,
            &self.shared_secret,
        )
    }

    /// Encode the `SelfVerificationData` into a `QrCode`.
    ///
    /// This method turns the `SelfVerificationData` into a QR code that can be
    /// rendered and presented to be scanned.
    ///
    /// The encoding can fail if the data doesn't fit into a QR code or if the
    /// keys that should be encoded into the QR code are not valid base64.
    pub fn to_qr_code(&self) -> Result<QrCode, EncodingError> {
        to_qr_code(
            Self::QR_MODE,
            &self.transaction_id,
            &self.master_key,
            &self.device_key,
            &self.shared_secret,
        )
    }
}

impl From<SelfVerificationData> for QrVerification {
    fn from(data: SelfVerificationData) -> Self {
        Self::SelfVerification(data)
    }
}

/// The non-encoded data for the third mode of QR code verification.
///
/// This mode is used for verification between two devices of the same user
/// where this device, that is creating this QR code, is not trusting the
/// cross signing master key.
#[derive(Clone, Debug, PartialEq)]
pub struct SelfVerificationNoMasterKey {
    transaction_id: String,
    device_key: String,
    master_key: String,
    shared_secret: String,
}

impl SelfVerificationNoMasterKey {
    const QR_MODE: u8 = 0x02;

    /// Create a new `SelfVerificationData` struct that can be encoded as a QR
    /// code.
    ///
    /// # Arguments
    /// * `transaction_id` - The transaction id of this verification flow, the
    /// transaction id was sent by the `m.key.verification.request` event
    /// that initiated the verification flow this QR code should be part of.
    ///
    /// * `device_key` - The ed25519 key of our own device, encoded as unpadded
    /// base64.
    ///
    /// * `master_key` - Our own cross signing master key. Needs to be encoded
    ///   as
    /// unpadded base64
    ///
    /// * ` shared_secret` - A random bytestring encoded as unpadded base64,
    /// needs to be at least 8 bytes long.
    pub fn new(
        transaction_id: String,
        device_key: String,
        master_key: String,
        shared_secret: String,
    ) -> Self {
        Self { transaction_id, device_key, master_key, shared_secret }
    }

    /// Encode the `SelfVerificationNoMasterKey` into a vector of bytes that can
    /// be encoded as a QR code.
    ///
    /// The encoding can fail if the keys that should be encoded are not valid
    /// base64.
    ///
    /// # Example
    /// ```
    /// # use matrix_qrcode::{QrVerification, DecodingError};
    /// # fn main() -> Result<(), DecodingError> {
    /// let data = b"MATRIX\
    ///              \x02\x02\x00\x06\
    ///              FLOWID\
    ///              AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
    ///              BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB\
    ///              SHARED_SECRET";
    ///
    /// let result = QrVerification::from_bytes(data)?;
    /// if let QrVerification::SelfVerificationNoMasterKey(decoded) = result {
    ///     let encoded = decoded.to_bytes().unwrap();
    ///     assert_eq!(data.as_ref(), encoded.as_slice());
    /// } else {
    ///     panic!("Data was encoded as an incorrect mode");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn to_bytes(&self) -> Result<Vec<u8>, EncodingError> {
        to_bytes(
            Self::QR_MODE,
            &self.transaction_id,
            &self.device_key,
            &self.master_key,
            &self.shared_secret,
        )
    }

    /// Encode the `SelfVerificationNoMasterKey` into a `QrCode`.
    ///
    /// This method turns the `SelfVerificationNoMasterKey` into a QR code that
    /// can be rendered and presented to be scanned.
    ///
    /// The encoding can fail if the data doesn't fit into a QR code or if the
    /// keys that should be encoded into the QR code are not valid base64.
    pub fn to_qr_code(&self) -> Result<QrCode, EncodingError> {
        to_qr_code(
            Self::QR_MODE,
            &self.transaction_id,
            &self.device_key,
            &self.master_key,
            &self.shared_secret,
        )
    }
}

impl From<SelfVerificationNoMasterKey> for QrVerification {
    fn from(data: SelfVerificationNoMasterKey) -> Self {
        Self::SelfVerificationNoMasterKey(data)
    }
}
