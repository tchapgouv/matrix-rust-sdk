use std::sync::Arc;

use base64::{decode_config, encode_config, STANDARD_NO_PAD};
use matrix_sdk_crypto::{
    matrix_sdk_qrcode::QrVerificationData, CancelInfo as RustCancelInfo, QrVerification as InnerQr,
    Sas as InnerSas, Verification as InnerVerification,
    VerificationRequest as InnerVerificationRequest,
};
use ruma::events::key::verification::VerificationMethod;
use tokio::runtime::Handle;

use crate::{CryptoStoreError, OutgoingVerificationRequest, SignatureUploadRequest};

/// Enum representing the different verification flows we support.
pub struct Verification {
    pub(crate) inner: InnerVerification,
    pub(crate) runtime: Handle,
}

impl Verification {
    /// Try to represent the `Verification` as an `Sas` verification object,
    /// returns `None` if the verification is not a `Sas` verification.
    pub fn as_sas(&self) -> Option<Arc<Sas>> {
        if let InnerVerification::SasV1(sas) = &self.inner {
            Some(Sas { inner: sas.to_owned(), runtime: self.runtime.to_owned() }.into())
        } else {
            None
        }
    }

    /// Try to represent the `Verification` as an `QrCode` verification object,
    /// returns `None` if the verification is not a `QrCode` verification.
    pub fn as_qr(&self) -> Option<Arc<QrCode>> {
        if let InnerVerification::QrV1(qr) = &self.inner {
            Some(QrCode { inner: qr.to_owned() }.into())
        } else {
            None
        }
    }
}

/// The `m.sas.v1` verification flow.
pub struct Sas {
    pub(crate) inner: InnerSas,
    pub(crate) runtime: Handle,
}

impl Sas {
    /// Get the user id of the other side.
    pub fn other_user_id(&self) -> String {
        self.inner.other_user_id().to_string()
    }

    /// Get the device ID of the other side.
    pub fn other_device_id(&self) -> String {
        self.inner.other_device_id().to_string()
    }

    /// Get the unique ID that identifies this SAS verification flow.
    pub fn flow_id(&self) -> String {
        self.inner.flow_id().as_str().to_owned()
    }

    /// Get the room id if the verification is happening inside a room.
    pub fn room_id(&self) -> Option<String> {
        self.inner.room_id().map(|r| r.to_string())
    }

    /// Is the SAS flow done.
    pub fn is_done(&self) -> bool {
        self.inner.is_done()
    }

    /// Did we initiate the verification flow.
    pub fn we_started(&self) -> bool {
        self.inner.we_started()
    }

    /// Accept that we're going forward with the short auth string verification.
    pub fn accept(&self) -> Option<OutgoingVerificationRequest> {
        self.inner.accept().map(|r| r.into())
    }

    /// Confirm a verification was successful.
    ///
    /// This method should be called if a short auth string should be confirmed
    /// as matching.
    pub fn confirm(&self) -> Result<Option<ConfirmVerificationResult>, CryptoStoreError> {
        let (requests, signature_request) = self.runtime.block_on(self.inner.confirm())?;

        let requests = requests.into_iter().map(|r| r.into()).collect();

        Ok(Some(ConfirmVerificationResult {
            requests,
            signature_request: signature_request.map(|s| s.into()),
        }))
    }

    /// Cancel the SAS verification using the given cancel code.
    ///
    /// # Arguments
    ///
    /// * `cancel_code` - The error code for why the verification was cancelled,
    /// manual cancellatio usually happens with `m.user` cancel code. The full
    /// list of cancel codes can be found in the [spec]
    ///
    /// [spec]: https://spec.matrix.org/unstable/client-server-api/#mkeyverificationcancel
    pub fn cancel(&self, cancel_code: &str) -> Option<OutgoingVerificationRequest> {
        self.inner.cancel_with_code(cancel_code.into()).map(|r| r.into())
    }

    /// Get a list of emoji indices of the emoji representation of the short
    /// auth string.
    ///
    /// *Note*: A SAS verification needs to be started and in the presentable
    /// state for this to return the list of emoji indices, otherwise returns
    /// `None`.
    pub fn get_emoji_indices(&self) -> Option<Vec<i32>> {
        self.inner.emoji_index().map(|v| v.iter().map(|i| (*i).into()).collect())
    }

    /// Get the decimal representation of the short auth string.
    ///
    /// *Note*: A SAS verification needs to be started and in the presentable
    /// state for this to return the list of decimals, otherwise returns
    /// `None`.
    pub fn get_decimals(&self) -> Option<Vec<i32>> {
        self.inner.decimals().map(|v| [v.0.into(), v.1.into(), v.2.into()].to_vec())
    }
}

/// The `m.qr_code.scan.v1`, `m.qr_code.show.v1`, and `m.reciprocate.v1`
/// verification flow.
pub struct QrCode {
    pub(crate) inner: InnerQr,
}

impl QrCode {
    /// Get the user id of the other side.
    pub fn other_user_id(&self) -> String {
        self.inner.other_user_id().to_string()
    }

    /// Get the device ID of the other side.
    pub fn other_device_id(&self) -> String {
        self.inner.other_device_id().to_string()
    }

    /// Get the unique ID that identifies this QR code verification flow.
    pub fn flow_id(&self) -> String {
        self.inner.flow_id().as_str().to_owned()
    }

    /// Get the room id if the verification is happening inside a room.
    pub fn room_id(&self) -> Option<String> {
        self.inner.room_id().map(|r| r.to_string())
    }

    /// Is the QR code verification done.
    pub fn is_done(&self) -> bool {
        self.inner.is_done()
    }

    /// Did we initiate the verification flow.
    pub fn we_started(&self) -> bool {
        self.inner.we_started()
    }

    /// Get the CancelInfo of this QR code verification object.
    ///
    /// Will be `None` if the flow has not been cancelled.
    pub fn cancel_info(&self) -> Option<CancelInfo> {
        self.inner.cancel_info().map(|c| c.into())
    }

    /// Has the QR verification been scanned by the other side.
    ///
    /// When the verification object is in this state it's required that the
    /// user confirms that the other side has scanned the QR code.
    pub fn has_been_scanned(&self) -> bool {
        self.inner.has_been_scanned()
    }

    /// Have we successfully scanned the QR code and are able to send a
    /// reciprocation event.
    pub fn reciprocated(&self) -> bool {
        self.inner.reciprocated()
    }

    /// Cancel the QR code verification using the given cancel code.
    ///
    /// # Arguments
    ///
    /// * `cancel_code` - The error code for why the verification was cancelled,
    /// manual cancellatio usually happens with `m.user` cancel code. The full
    /// list of cancel codes can be found in the [spec]
    ///
    /// [spec]: https://spec.matrix.org/unstable/client-server-api/#mkeyverificationcancel
    pub fn cancel(&self, cancel_code: &str) -> Option<OutgoingVerificationRequest> {
        self.inner.cancel_with_code(cancel_code.into()).map(|r| r.into())
    }

    /// Confirm a verification was successful.
    ///
    /// This method should be called if we want to confirm that the other side
    /// has scanned our QR code.
    pub fn confirm(&self) -> Option<ConfirmVerificationResult> {
        self.inner.confirm_scanning().map(|r| ConfirmVerificationResult {
            requests: vec![r.into()],
            signature_request: None,
        })
    }

    /// Generate data that should be encoded as a QR code.
    ///
    /// This method should be called right before a QR code should be displayed,
    /// the returned data is base64 encoded (without padding) and needs to be
    /// decoded on the other side before it can be put through a QR code
    /// generator.
    pub fn generate_qr_code(&self) -> Option<String> {
        self.inner.to_bytes().map(|data| encode_config(data, STANDARD_NO_PAD)).ok()
    }
}

/// Information on why a verification flow has been cancelled and by whom.
pub struct CancelInfo {
    /// The textual representation of the cancel reason
    pub reason: String,
    /// The code describing the cancel reason
    pub cancel_code: String,
    /// Was the verification flow cancelled by us
    pub cancelled_by_us: bool,
}

impl From<RustCancelInfo> for CancelInfo {
    fn from(c: RustCancelInfo) -> Self {
        Self {
            reason: c.reason().to_owned(),
            cancel_code: c.cancel_code().to_string(),
            cancelled_by_us: c.cancelled_by_us(),
        }
    }
}

/// A result type for starting SAS verifications.
pub struct StartSasResult {
    /// The SAS verification object that got created.
    pub sas: Arc<Sas>,
    /// The request that needs to be sent out to notify the other side that a
    /// SAS verification should start.
    pub request: OutgoingVerificationRequest,
}

/// A result type for scanning QR codes.
pub struct ScanResult {
    /// The QR code verification object that got created.
    pub qr: Arc<QrCode>,
    /// The request that needs to be sent out to notify the other side that a
    /// QR code verification should start.
    pub request: OutgoingVerificationRequest,
}

/// A result type for requesting verifications.
pub struct RequestVerificationResult {
    /// The verification request object that got created.
    pub verification: Arc<VerificationRequest>,
    /// The request that needs to be sent out to notify the other side that
    /// we're requesting verification to begin.
    pub request: OutgoingVerificationRequest,
}

/// A result type for confirming verifications.
pub struct ConfirmVerificationResult {
    /// The requests that needs to be sent out to notify the other side that we
    /// confirmed the verification.
    pub requests: Vec<OutgoingVerificationRequest>,
    /// A request that will upload signatures of the verified device or user, if
    /// the verification is completed and we're able to sign devices or users
    pub signature_request: Option<SignatureUploadRequest>,
}

/// The verificatoin request object which then can transition into some concrete
/// verification method
pub struct VerificationRequest {
    pub(crate) inner: InnerVerificationRequest,
    pub(crate) runtime: Handle,
}

impl VerificationRequest {
    /// The id of the other user that is participating in this verification
    /// request.
    pub fn other_user_id(&self) -> String {
        self.inner.other_user().to_string()
    }

    /// The id of the other device that is participating in this verification.
    pub fn other_device_id(&self) -> Option<String> {
        self.inner.other_device_id().map(|d| d.to_string())
    }

    /// Get the unique ID of this verification request
    pub fn flow_id(&self) -> String {
        self.inner.flow_id().as_str().to_owned()
    }

    /// Get the room id if the verification is happening inside a room.
    pub fn room_id(&self) -> Option<String> {
        self.inner.room_id().map(|r| r.to_string())
    }

    /// Has the verification flow that was started with this request finished.
    pub fn is_done(&self) -> bool {
        self.inner.is_done()
    }

    /// Is the verification request ready to start a verification flow.
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    /// Did we initiate the verification request
    pub fn we_started(&self) -> bool {
        self.inner.we_started()
    }

    /// Has the verification request been answered by another device.
    pub fn is_passive(&self) -> bool {
        self.inner.is_passive()
    }

    /// Get the supported verification methods of the other side.
    ///
    /// Will be present only if the other side requested the verification or if
    /// we're in the ready state.
    pub fn their_supported_methods(&self) -> Option<Vec<String>> {
        self.inner.their_supported_methods().map(|m| m.iter().map(|m| m.to_string()).collect())
    }

    /// Get our own supported verification methods that we advertised.
    ///
    /// Will be present only we requested the verification or if we're in the
    /// ready state.
    pub fn our_supported_methods(&self) -> Option<Vec<String>> {
        self.inner.our_supported_methods().map(|m| m.iter().map(|m| m.to_string()).collect())
    }

    /// Accept a verification requests that we share with the given user with
    /// the given flow id.
    ///
    /// This will move the verification request into the ready state.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The ID of the user for which we would like to accept the
    /// verification requests.
    ///
    /// * `flow_id` - The ID that uniquely identifies the verification flow.
    ///
    /// * `methods` - A list of verification methods that we want to advertise
    /// as supported.
    pub fn accept(&self, methods: Vec<String>) -> Option<OutgoingVerificationRequest> {
        let methods = methods.into_iter().map(VerificationMethod::from).collect();
        self.inner.accept_with_methods(methods).map(|r| r.into())
    }

    /// Cancel a verification for the given user with the given flow id using
    /// the given cancel code.
    pub fn cancel(&self) -> Option<OutgoingVerificationRequest> {
        self.inner.cancel().map(|r| r.into())
    }

    /// Transition from a verification request into short auth string based
    /// verification.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The ID of the user for which we would like to start the
    /// SAS verification.
    ///
    /// * `flow_id` - The ID of the verification request that initiated the
    /// verification flow.
    pub fn start_sas_verification(&self) -> Result<Option<StartSasResult>, CryptoStoreError> {
        Ok(self.runtime.block_on(self.inner.start_sas())?.map(|(sas, r)| StartSasResult {
            sas: Arc::new(Sas { inner: sas, runtime: self.runtime.clone() }),
            request: r.into(),
        }))
    }

    /// Transition from a verification request into QR code verification.
    ///
    /// This method should be called when one wants to display a QR code so the
    /// other side can scan it and move the QR code verification forward.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The ID of the user for which we would like to start the
    /// QR code verification.
    ///
    /// * `flow_id` - The ID of the verification request that initiated the
    /// verification flow.
    pub fn start_qr_verification(&self) -> Result<Option<Arc<QrCode>>, CryptoStoreError> {
        Ok(self
            .runtime
            .block_on(self.inner.generate_qr_code())?
            .map(|qr| QrCode { inner: qr }.into()))
    }

    /// Pass data from a scanned QR code to an active verification request and
    /// transition into QR code verification.
    ///
    /// This requires an active `VerificationRequest` to succeed, returns `None`
    /// if no `VerificationRequest` is found or if the QR code data is invalid.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The ID of the user for which we would like to start the
    /// QR code verification.
    ///
    /// * `flow_id` - The ID of the verification request that initiated the
    /// verification flow.
    ///
    /// * `data` - The data that was extracted from the scanned QR code as an
    /// base64 encoded string, without padding.
    pub fn scan_qr_code(&self, data: &str) -> Option<ScanResult> {
        let data = decode_config(data, STANDARD_NO_PAD).ok()?;
        let data = QrVerificationData::from_bytes(data).ok()?;

        if let Some(qr) = self.runtime.block_on(self.inner.scan_qr_code(data)).ok()? {
            let request = qr.reciprocate()?;

            Some(ScanResult { qr: QrCode { inner: qr }.into(), request: request.into() })
        } else {
            None
        }
    }
}
