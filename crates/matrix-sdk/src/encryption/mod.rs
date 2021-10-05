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

//! End to end encryption related types

pub mod identities;
pub mod verification;
use std::{
    collections::{BTreeMap, HashSet},
    io::Write,
    path::PathBuf,
    result::Result as StdResult,
};

use futures::StreamExt;
pub use matrix_sdk_base::crypto::{EncryptionInfo, LocalTrust};
use matrix_sdk_base::{
    crypto::{
        store::CryptoStoreError, CrossSigningStatus, OutgoingRequest, RoomMessageRequest,
        ToDeviceRequest,
    },
    deserialized_responses::RoomEvent,
};
use matrix_sdk_common::{instant::Duration, uuid::Uuid};
use ruma::{
    api::client::r0::{
        keys::{get_keys, upload_keys, upload_signing_keys::Request as UploadSigningKeysRequest},
        message::send_message_event,
        to_device::send_event_to_device::{
            Request as RumaToDeviceRequest, Response as ToDeviceResponse,
        },
        uiaa::AuthData,
    },
    assign,
    events::{AnyMessageEvent, AnyRoomEvent, AnySyncMessageEvent, EventType},
    serde::Raw,
    DeviceId, DeviceIdBox, UserId,
};
use tracing::{debug, instrument, trace, warn};

use crate::{
    encryption::{
        identities::{Device, UserDevices},
        verification::{SasVerification, Verification, VerificationRequest},
    },
    error::{HttpError, HttpResult, RoomKeyImportError},
    room, Client, Error, Result,
};

impl Client {
    /// Get the public ed25519 key of our own device. This is usually what is
    /// called the fingerprint of the device.
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    pub async fn ed25519_key(&self) -> Option<String> {
        self.base_client.olm_machine().await.map(|o| o.identity_keys().ed25519().to_owned())
    }

    /// Get the status of the private cross signing keys.
    ///
    /// This can be used to check which private cross signing keys we have
    /// stored locally.
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    pub async fn cross_signing_status(&self) -> Option<CrossSigningStatus> {
        if let Some(machine) = self.base_client.olm_machine().await {
            Some(machine.cross_signing_status().await)
        } else {
            None
        }
    }

    /// Get all the tracked users we know about
    ///
    /// Tracked users are users for which we keep the device list of E2EE
    /// capable devices up to date.
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    pub async fn tracked_users(&self) -> HashSet<UserId> {
        self.base_client.olm_machine().await.map(|o| o.tracked_users()).unwrap_or_default()
    }

    /// Get a verification object with the given flow id.
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    pub async fn get_verification(&self, user_id: &UserId, flow_id: &str) -> Option<Verification> {
        let olm = self.base_client.olm_machine().await?;
        olm.get_verification(user_id, flow_id).map(|v| match v {
            matrix_sdk_base::crypto::Verification::SasV1(s) => {
                SasVerification { inner: s, client: self.clone() }.into()
            }
            #[cfg(feature = "qrcode")]
            matrix_sdk_base::crypto::Verification::QrV1(qr) => {
                verification::QrVerification { inner: qr, client: self.clone() }.into()
            }
        })
    }

    /// Get a `VerificationRequest` object for the given user with the given
    /// flow id.
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    pub async fn get_verification_request(
        &self,
        user_id: &UserId,
        flow_id: impl AsRef<str>,
    ) -> Option<VerificationRequest> {
        let olm = self.base_client.olm_machine().await?;

        olm.get_verification_request(user_id, flow_id)
            .map(|r| VerificationRequest { inner: r, client: self.clone() })
    }

    /// Get a specific device of a user.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The unique id of the user that the device belongs to.
    ///
    /// * `device_id` - The unique id of the device.
    ///
    /// Returns a `Device` if one is found and the crypto store didn't throw an
    /// error.
    ///
    /// This will always return None if the client hasn't been logged in.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::convert::TryFrom;
    /// # use matrix_sdk::{Client, ruma::UserId};
    /// # use url::Url;
    /// # use futures::executor::block_on;
    /// # let alice = UserId::try_from("@alice:example.org").unwrap();
    /// # let homeserver = Url::parse("http://example.com").unwrap();
    /// # let client = Client::new(homeserver).unwrap();
    /// # block_on(async {
    /// let device = client.get_device(&alice, "DEVICEID".into())
    ///     .await
    ///     .unwrap()
    ///     .unwrap();
    ///
    /// println!("{:?}", device.verified());
    ///
    /// let verification = device.request_verification().await.unwrap();
    /// # });
    /// ```
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    pub async fn get_device(
        &self,
        user_id: &UserId,
        device_id: &DeviceId,
    ) -> StdResult<Option<Device>, CryptoStoreError> {
        let device = self.base_client.get_device(user_id, device_id).await?;

        Ok(device.map(|d| Device { inner: d, client: self.clone() }))
    }

    /// Get a map holding all the devices of an user.
    ///
    /// This will always return an empty map if the client hasn't been logged
    /// in.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The unique id of the user that the devices belong to.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::convert::TryFrom;
    /// # use matrix_sdk::{Client, ruma::UserId};
    /// # use url::Url;
    /// # use futures::executor::block_on;
    /// # let alice = UserId::try_from("@alice:example.org").unwrap();
    /// # let homeserver = Url::parse("http://example.com").unwrap();
    /// # let client = Client::new(homeserver).unwrap();
    /// # block_on(async {
    /// let devices = client.get_user_devices(&alice).await.unwrap();
    ///
    /// for device in devices.devices() {
    ///     println!("{:?}", device);
    /// }
    /// # });
    /// ```
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    pub async fn get_user_devices(
        &self,
        user_id: &UserId,
    ) -> StdResult<UserDevices, CryptoStoreError> {
        let devices = self.base_client.get_user_devices(user_id).await?;

        Ok(UserDevices { inner: devices, client: self.clone() })
    }

    /// Get a E2EE identity of an user.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The unique id of the user that the identity belongs to.
    ///
    /// Returns a `UserIdentity` if one is found and the crypto store
    /// didn't throw an error.
    ///
    /// This will always return None if the client hasn't been logged in.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::convert::TryFrom;
    /// # use matrix_sdk::{Client, ruma::UserId};
    /// # use url::Url;
    /// # use futures::executor::block_on;
    /// # let alice = UserId::try_from("@alice:example.org").unwrap();
    /// # let homeserver = Url::parse("http://example.com").unwrap();
    /// # let client = Client::new(homeserver).unwrap();
    /// # block_on(async {
    /// let user = client.get_user_identity(&alice).await?;
    ///
    /// if let Some(user) = user {
    ///     println!("{:?}", user.verified());
    ///
    ///     let verification = user.request_verification().await?;
    /// }
    /// # anyhow::Result::<()>::Ok(()) });
    /// ```
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    pub async fn get_user_identity(
        &self,
        user_id: &UserId,
    ) -> StdResult<Option<crate::encryption::identities::UserIdentity>, CryptoStoreError> {
        use crate::encryption::identities::UserIdentity;

        if let Some(olm) = self.base_client.olm_machine().await {
            let identity = olm.get_identity(user_id).await?;

            Ok(identity.map(|i| match i {
                matrix_sdk_base::crypto::UserIdentities::Own(i) => {
                    UserIdentity::new_own(self.clone(), i)
                }
                matrix_sdk_base::crypto::UserIdentities::Other(i) => {
                    UserIdentity::new(self.clone(), i, self.get_dm_room(user_id))
                }
            }))
        } else {
            Ok(None)
        }
    }

    /// Create and upload a new cross signing identity.
    ///
    /// # Arguments
    ///
    /// * `auth_data` - This request requires user interactive auth, the first
    /// request needs to set this to `None` and will always fail with an
    /// `UiaaResponse`. The response will contain information for the
    /// interactive auth and the same request needs to be made but this time
    /// with some `auth_data` provided.
    ///
    /// # Examples
    /// ```no_run
    /// # use std::{convert::TryFrom, collections::BTreeMap};
    /// # use matrix_sdk::{
    /// #     ruma::{api::client::r0::uiaa, assign, UserId},
    /// #     Client,
    /// # };
    /// # use url::Url;
    /// # use futures::executor::block_on;
    /// # use serde_json::json;
    /// # let user_id = UserId::try_from("@alice:example.org").unwrap();
    /// # let homeserver = Url::parse("http://example.com").unwrap();
    /// # let client = Client::new(homeserver).unwrap();
    /// # block_on(async {
    /// if let Err(e) = client.bootstrap_cross_signing(None).await {
    ///     if let Some(response) = e.uiaa_response() {
    ///         let auth_data = uiaa::AuthData::Password(assign!(
    ///             uiaa::Password::new(uiaa::UserIdentifier::MatrixId("example"), "wordpass"),
    ///             { session: response.session.as_deref() }
    ///         ));
    ///
    ///         client
    ///             .bootstrap_cross_signing(Some(auth_data))
    ///             .await
    ///             .expect("Couldn't bootstrap cross signing")
    ///     } else {
    ///         panic!("Error durign cross signing bootstrap {:#?}", e);
    ///     }
    /// }
    /// # })
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    pub async fn bootstrap_cross_signing(&self, auth_data: Option<AuthData<'_>>) -> Result<()> {
        let olm = self.base_client.olm_machine().await.ok_or(Error::AuthenticationRequired)?;

        let (request, signature_request) = olm.bootstrap_cross_signing(false).await?;

        let request = assign!(UploadSigningKeysRequest::new(), {
            auth: auth_data,
            master_key: request.master_key,
            self_signing_key: request.self_signing_key,
            user_signing_key: request.user_signing_key,
        });

        self.send(request, None).await?;
        self.send(signature_request, None).await?;

        Ok(())
    }

    /// Export E2EE keys that match the given predicate encrypting them with the
    /// given passphrase.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path where the exported key file will be saved.
    ///
    /// * `passphrase` - The passphrase that will be used to encrypt the
    ///   exported
    /// room keys.
    ///
    /// * `predicate` - A closure that will be called for every known
    /// `InboundGroupSession`, which represents a room key. If the closure
    /// returns `true` the `InboundGroupSessoin` will be included in the export,
    /// if the closure returns `false` it will not be included.
    ///
    /// # Panics
    ///
    /// This method will panic if it isn't run on a Tokio runtime.
    ///
    /// This method will panic if it can't get enough randomness from the OS to
    /// encrypt the exported keys securely.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::{path::PathBuf, time::Duration};
    /// # use matrix_sdk::{
    /// #     Client, config::SyncSettings,
    /// #     ruma::room_id,
    /// # };
    /// # use futures::executor::block_on;
    /// # use url::Url;
    /// # block_on(async {
    /// # let homeserver = Url::parse("http://localhost:8080").unwrap();
    /// # let mut client = Client::new(homeserver).unwrap();
    /// let path = PathBuf::from("/home/example/e2e-keys.txt");
    /// // Export all room keys.
    /// client
    ///     .export_keys(path, "secret-passphrase", |_| true)
    ///     .await
    ///     .expect("Can't export keys.");
    ///
    /// // Export only the room keys for a certain room.
    /// let path = PathBuf::from("/home/example/e2e-room-keys.txt");
    /// let room_id = room_id!("!test:localhost");
    ///
    /// client
    ///     .export_keys(path, "secret-passphrase", |s| s.room_id() == &room_id)
    ///     .await
    ///     .expect("Can't export keys.");
    /// # });
    /// ```
    #[cfg(all(feature = "encryption", not(target_arch = "wasm32")))]
    #[cfg_attr(feature = "docs", doc(cfg(all(encryption, not(target_arch = "wasm32")))))]
    pub async fn export_keys(
        &self,
        path: PathBuf,
        passphrase: &str,
        predicate: impl FnMut(&matrix_sdk_base::crypto::olm::InboundGroupSession) -> bool,
    ) -> Result<()> {
        let olm = self.base_client.olm_machine().await.ok_or(Error::AuthenticationRequired)?;

        let keys = olm.export_keys(predicate).await?;
        let passphrase = zeroize::Zeroizing::new(passphrase.to_owned());

        let encrypt = move || -> Result<()> {
            let export: String =
                matrix_sdk_base::crypto::encrypt_key_export(&keys, &passphrase, 500_000)?;
            let mut file = std::fs::File::create(path)?;
            file.write_all(&export.into_bytes())?;
            Ok(())
        };

        let task = tokio::task::spawn_blocking(encrypt);
        task.await.expect("Task join error")
    }

    /// Import E2EE keys from the given file path.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path where the exported key file will can be found.
    ///
    /// * `passphrase` - The passphrase that should be used to decrypt the
    /// exported room keys.
    ///
    /// Returns a tuple of numbers that represent the number of sessions that
    /// were imported and the total number of sessions that were found in the
    /// key export.
    ///
    /// # Panics
    ///
    /// This method will panic if it isn't run on a Tokio runtime.
    ///
    /// ```no_run
    /// # use std::{path::PathBuf, time::Duration};
    /// # use matrix_sdk::{
    /// #     Client, config::SyncSettings,
    /// #     ruma::room_id,
    /// # };
    /// # use futures::executor::block_on;
    /// # use url::Url;
    /// # block_on(async {
    /// # let homeserver = Url::parse("http://localhost:8080").unwrap();
    /// # let mut client = Client::new(homeserver).unwrap();
    /// let path = PathBuf::from("/home/example/e2e-keys.txt");
    /// client
    ///     .import_keys(path, "secret-passphrase")
    ///     .await
    ///     .expect("Can't import keys");
    /// # });
    /// ```
    #[cfg(all(feature = "encryption", not(target_arch = "wasm32")))]
    #[cfg_attr(feature = "docs", doc(cfg(all(encryption, not(target_arch = "wasm32")))))]
    pub async fn import_keys(
        &self,
        path: PathBuf,
        passphrase: &str,
    ) -> StdResult<(usize, usize), RoomKeyImportError> {
        let olm = self.base_client.olm_machine().await.ok_or(RoomKeyImportError::StoreClosed)?;
        let passphrase = zeroize::Zeroizing::new(passphrase.to_owned());

        let decrypt = move || {
            let file = std::fs::File::open(path)?;
            matrix_sdk_base::crypto::decrypt_key_export(file, &passphrase)
        };

        let task = tokio::task::spawn_blocking(decrypt);
        let import = task.await.expect("Task join error")?;

        Ok(olm.import_keys(import, |_, _| {}).await?)
    }

    /// Tries to decrypt a `AnyRoomEvent`. Returns unencrypted room event when
    /// decryption fails.
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    pub(crate) async fn decrypt_room_event(&self, event: &AnyRoomEvent) -> RoomEvent {
        if let Some(machine) = self.base_client.olm_machine().await {
            if let AnyRoomEvent::Message(event) = event {
                if let AnyMessageEvent::RoomEncrypted(_) = event {
                    let room_id = event.room_id();
                    // Turn the AnyMessageEvent into a AnySyncMessageEvent
                    let event = event.clone().into();

                    if let AnySyncMessageEvent::RoomEncrypted(e) = event {
                        if let Ok(decrypted) = machine.decrypt_room_event(&e, room_id).await {
                            let event = Raw::new(
                                &decrypted
                                    .event
                                    .deserialize()
                                    .unwrap()
                                    .into_full_event(room_id.clone()),
                            )
                            .expect("Failed to serialize event");
                            let encryption_info = decrypted.encryption_info;

                            // Return decrypted room event
                            return RoomEvent { event, encryption_info };
                        }
                    }
                }
            }
        }

        // Fallback to still-encrypted room event
        RoomEvent { event: Raw::new(event).expect("Failed to serialize "), encryption_info: None }
    }

    /// Query the server for users device keys.
    ///
    /// # Panics
    ///
    /// Panics if no key query needs to be done.
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    #[instrument]
    pub(crate) async fn keys_query(
        &self,
        request_id: &Uuid,
        device_keys: BTreeMap<UserId, Vec<DeviceIdBox>>,
    ) -> Result<get_keys::Response> {
        let request = assign!(get_keys::Request::new(), { device_keys });

        let response = self.send(request, None).await?;
        self.base_client.mark_request_as_sent(request_id, &response).await?;

        Ok(response)
    }

    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    async fn send_account_data(
        &self,
        content: ruma::events::AnyGlobalAccountDataEventContent,
    ) -> Result<ruma::api::client::r0::config::set_global_account_data::Response> {
        let own_user =
            self.user_id().await.ok_or_else(|| Error::from(HttpError::AuthenticationRequired))?;
        let data = serde_json::value::to_raw_value(&content)?;

        let request = ruma::api::client::r0::config::set_global_account_data::Request::new(
            &data,
            ruma::events::EventContent::event_type(&content),
            &own_user,
        );

        Ok(self.send(request, None).await?)
    }

    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    pub(crate) async fn create_dm_room(&self, user_id: UserId) -> Result<Option<room::Joined>> {
        use ruma::{
            api::client::r0::room::create_room::RoomPreset,
            events::AnyGlobalAccountDataEventContent,
        };

        const SYNC_WAIT_TIME: Duration = Duration::from_secs(3);

        // First we create the DM room, where we invite the user and tell the
        // invitee that the room should be a DM.
        let invite = &[user_id.clone()];

        let request = assign!(
            ruma::api::client::r0::room::create_room::Request::new(),
            {
                invite,
                is_direct: true,
                preset: Some(RoomPreset::TrustedPrivateChat),
            }
        );

        let response = self.send(request, None).await?;

        // Now we need to mark the room as a DM for ourselves, we fetch the
        // existing `m.direct` event and append the room to the list of DMs we
        // have with this user.
        let mut content = self
            .store()
            .get_account_data_event(EventType::Direct)
            .await?
            .map(|e| e.deserialize())
            .transpose()?
            .and_then(|e| {
                if let AnyGlobalAccountDataEventContent::Direct(c) = e.content() {
                    Some(c)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| ruma::events::direct::DirectEventContent(BTreeMap::new()));

        content.entry(user_id.to_owned()).or_default().push(response.room_id.to_owned());

        // TODO We should probably save the fact that we need to send this out
        // because otherwise we might end up in a state where we have a DM that
        // isn't marked as one.
        self.send_account_data(AnyGlobalAccountDataEventContent::Direct(content)).await?;

        // If the room is already in our store, fetch it, otherwise wait for a
        // sync to be done which should put the room into our store.
        if let Some(room) = self.get_joined_room(&response.room_id) {
            Ok(Some(room))
        } else {
            self.sync_beat.listen().wait_timeout(SYNC_WAIT_TIME);
            Ok(self.get_joined_room(&response.room_id))
        }
    }

    /// Claim one-time keys creating new Olm sessions.
    ///
    /// # Arguments
    ///
    /// * `users` - The list of user/device pairs that we should claim keys for.
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    #[instrument(skip(users))]
    pub(crate) async fn claim_one_time_keys(
        &self,
        users: impl Iterator<Item = &UserId>,
    ) -> Result<()> {
        let _lock = self.key_claim_lock.lock().await;

        if let Some((request_id, request)) = self.base_client.get_missing_sessions(users).await? {
            let response = self.send(request, None).await?;
            self.base_client.mark_request_as_sent(&request_id, &response).await?;
        }

        Ok(())
    }

    /// Upload the E2E encryption keys.
    ///
    /// This uploads the long lived device keys as well as the required amount
    /// of one-time keys.
    ///
    /// # Panics
    ///
    /// Panics if the client isn't logged in, or if no encryption keys need to
    /// be uploaded.
    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    #[instrument]
    pub(crate) async fn keys_upload(
        &self,
        request_id: &Uuid,
        request: &upload_keys::Request,
    ) -> Result<upload_keys::Response> {
        debug!(
            "Uploading encryption keys device keys: {}, one-time-keys: {}",
            request.device_keys.is_some(),
            request.one_time_keys.as_ref().map_or(0, |k| k.len())
        );

        let response = self.send(request.clone(), None).await?;
        self.base_client.mark_request_as_sent(request_id, &response).await?;

        Ok(response)
    }

    #[cfg(feature = "encryption")]
    pub(crate) async fn room_send_helper(
        &self,
        request: &RoomMessageRequest,
    ) -> Result<send_message_event::Response> {
        let content = request.content.clone();
        let txn_id = request.txn_id;
        let room_id = &request.room_id;

        self.get_joined_room(room_id)
            .expect("Can't send a message to a room that isn't known to the store")
            .send(content, Some(txn_id))
            .await
    }

    #[cfg(feature = "encryption")]
    pub(crate) async fn send_to_device(
        &self,
        request: &ToDeviceRequest,
    ) -> HttpResult<ToDeviceResponse> {
        let txn_id_string = request.txn_id_string();

        let request = RumaToDeviceRequest::new_raw(
            request.event_type.as_str(),
            &txn_id_string,
            request.messages.clone(),
        );

        self.send(request, None).await
    }

    #[cfg(feature = "encryption")]
    pub(crate) async fn send_verification_request(
        &self,
        request: matrix_sdk_base::crypto::OutgoingVerificationRequest,
    ) -> Result<()> {
        match request {
            matrix_sdk_base::crypto::OutgoingVerificationRequest::ToDevice(t) => {
                self.send_to_device(&t).await?;
            }
            matrix_sdk_base::crypto::OutgoingVerificationRequest::InRoom(r) => {
                self.room_send_helper(&r).await?;
            }
        }

        Ok(())
    }

    #[cfg(feature = "encryption")]
    #[cfg_attr(feature = "docs", doc(cfg(encryption)))]
    fn get_dm_room(&self, user_id: &UserId) -> Option<room::Joined> {
        let rooms = self.joined_rooms();
        let room_pairs: Vec<_> =
            rooms.iter().map(|r| (r.room_id().to_owned(), r.direct_target())).collect();
        trace!(rooms =? room_pairs, "Finding direct room");

        let room = rooms.into_iter().find(|r| r.direct_target().as_ref() == Some(user_id));

        trace!(room =? room, "Found room");
        room
    }

    async fn send_outgoing_request(&self, r: OutgoingRequest) -> Result<()> {
        use matrix_sdk_base::crypto::OutgoingRequests;

        match r.request() {
            OutgoingRequests::KeysQuery(request) => {
                self.keys_query(r.request_id(), request.device_keys.clone()).await?;
            }
            OutgoingRequests::KeysUpload(request) => {
                self.keys_upload(r.request_id(), request).await?;
            }
            OutgoingRequests::ToDeviceRequest(request) => {
                let response = self.send_to_device(request).await?;
                self.base_client.mark_request_as_sent(r.request_id(), &response).await?;
            }
            OutgoingRequests::SignatureUpload(request) => {
                let response = self.send(request.clone(), None).await?;
                self.base_client.mark_request_as_sent(r.request_id(), &response).await?;
            }
            OutgoingRequests::RoomMessage(request) => {
                let response = self.room_send_helper(request).await?;
                self.base_client.mark_request_as_sent(r.request_id(), &response).await?;
            }
            OutgoingRequests::KeysClaim(request) => {
                let response = self.send(request.clone(), None).await?;
                self.base_client.mark_request_as_sent(r.request_id(), &response).await?;
            }
        }

        Ok(())
    }

    pub(crate) async fn send_outgoing_requests(&self) -> Result<()> {
        const MAX_CONCURRENT_REQUESTS: usize = 20;

        // This is needed because sometimes we need to automatically
        // claim some one-time keys to unwedge an existing Olm session.
        if let Err(e) = self.claim_one_time_keys([].iter()).await {
            warn!("Error while claiming one-time keys {:?}", e);
        }

        let outgoing_requests = futures::stream::iter(self.base_client.outgoing_requests().await?)
            .map(|r| self.send_outgoing_request(r));

        let requests = outgoing_requests.buffer_unordered(MAX_CONCURRENT_REQUESTS);

        requests
            .for_each(|r| async move {
                match r {
                    Ok(_) => (),
                    Err(e) => warn!(errro =? e, "Error when sending out an outgoing E2EE request"),
                }
            })
            .await;

        Ok(())
    }
}
