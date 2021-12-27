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

//! Server-side backup support for room keys
//!
//! This module largely implements support for server-side backups using the
//! `m.megolm_backup.v1.curve25519-aes-sha2` backup algorithm.
//!
//! Due to various flaws in this backup algorithm it is **not** recommended to
//! use this module or any of its functionality. The module is only provided for
//! backwards compatibility.
//!
//! [spec]: https://spec.matrix.org/unstable/client-server-api/#server-side-key-backups

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use matrix_sdk_common::{locks::RwLock, uuid::Uuid};
use ruma::{
    api::client::r0::backup::RoomKeyBackup, serde::Raw, DeviceKeyAlgorithm, DeviceKeyId, RoomId,
    UserId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, instrument, trace, warn};

use crate::{
    olm::{Account, InboundGroupSession},
    store::{BackupKeys, Changes, RoomKeyCounts, Store},
    CryptoStoreError, KeysBackupRequest, OutgoingRequest,
};

mod keys;

pub use keys::{DecodeError, MegolmV1BackupKey, PickledRecoveryKey, RecoveryKey};
pub use olm_rs::errors::OlmPkDecryptionError;

/// A state machine that handles backing up room keys.
///
/// The state machine can be activated using the
/// [`BackupMachine::enable_backup_v1`] method. After the state machine has been
/// enabled a request that will upload encrypted room keys can be generated
/// using the [`BackupMachine::backup`] method.
#[derive(Debug, Clone)]
pub struct BackupMachine {
    account: Account,
    store: Store,
    backup_key: Arc<RwLock<Option<MegolmV1BackupKey>>>,
    pending_backup: Arc<RwLock<Option<PendingBackup>>>,
}

#[derive(Debug, Clone)]
struct PendingBackup {
    request_id: Uuid,
    request: KeysBackupRequest,
    sessions: BTreeMap<Box<RoomId>, BTreeMap<String, BTreeSet<String>>>,
}

impl PendingBackup {
    fn session_was_part_of_the_backup(&self, session: &InboundGroupSession) -> bool {
        self.sessions
            .get(session.room_id())
            .and_then(|r| r.get(session.sender_key()).map(|s| s.contains(session.session_id())))
            .unwrap_or(false)
    }
}

impl From<PendingBackup> for OutgoingRequest {
    fn from(b: PendingBackup) -> Self {
        OutgoingRequest { request_id: b.request_id, request: Arc::new(b.request.into()) }
    }
}

impl BackupMachine {
    const BACKUP_BATCH_SIZE: usize = 100;

    pub(crate) fn new(
        account: Account,
        store: Store,
        backup_key: Option<MegolmV1BackupKey>,
    ) -> Self {
        Self {
            account,
            store,
            backup_key: RwLock::new(backup_key).into(),
            pending_backup: RwLock::new(None).into(),
        }
    }

    /// Are we able to back up room keys to the server?
    pub async fn enabled(&self) -> bool {
        self.backup_key.read().await.as_ref().map(|b| b.backup_version().is_some()).unwrap_or(false)
    }

    /// Verify some backup auth data that we downloaded from the server.
    ///
    /// The auth data should be fetched from the server using the
    /// [`/room_keys/version`] endpoint.
    ///
    /// Ruma models this using the [`BackupAlgorithm`] struct, but care needs to
    /// be taken if that struct is used. Some clients might use unspecced fields
    /// and the given Ruma struct will lose the unspecced fields after a
    /// serialization cycle.
    ///
    /// [`BackupAlgorithm`]: ruma::api::client::r0::backup::BackupAlgorithm
    /// [`/room_keys/version`]: https://spec.matrix.org/unstable/client-server-api/#get_matrixclientv3room_keysversion
    pub async fn verify_backup(
        &self,
        mut serialized_auth_data: Value,
    ) -> Result<bool, CryptoStoreError> {
        #[derive(Debug, Serialize, Deserialize)]
        struct AuthData {
            public_key: String,
            #[serde(default)]
            signatures: BTreeMap<Box<UserId>, BTreeMap<Box<DeviceKeyId>, String>>,
            #[serde(flatten)]
            extra: BTreeMap<String, Value>,
        }

        let auth_data: AuthData = serde_json::from_value(serialized_auth_data.clone())?;

        trace!(?auth_data, "Verifying backup auth data");

        Ok(if let Some(signatures) = auth_data.signatures.get(self.store.user_id()) {
            for device_key_id in signatures.keys() {
                if device_key_id.algorithm() == DeviceKeyAlgorithm::Ed25519 {
                    if device_key_id.device_id() == self.account.device_id() {
                        let result = self.account.is_signed(&mut serialized_auth_data);

                        trace!(?result, "Checking auth data signature of our own device");

                        if result.is_ok() {
                            return Ok(true);
                        }
                    } else {
                        let device = self
                            .store
                            .get_device(self.store.user_id(), device_key_id.device_id())
                            .await?;

                        trace!(
                            device_id = device_key_id.device_id().as_str(),
                            "Checking backup auth data for device"
                        );

                        if let Some(device) = device {
                            if device.verified()
                                && device.is_signed_by_device(&mut serialized_auth_data).is_ok()
                            {
                                return Ok(true);
                            }
                        } else {
                            trace!(
                                device_id = device_key_id.device_id().as_str(),
                                "Device not found, can't check signature"
                            );
                        }
                    }
                }
            }

            false
        } else {
            false
        })
    }

    /// Activate the given backup key to be used to encrypt and backup room
    /// keys.
    ///
    /// This will use the [`m.megolm_backup.v1.curve25519-aes-sha2`] algorithm
    /// to encrypt the room keys.
    ///
    /// [`m.megolm_backup.v1.curve25519-aes-sha2`]:
    /// https://spec.matrix.org/unstable/client-server-api/#backup-algorithm-mmegolm_backupv1curve25519-aes-sha2
    pub async fn enable_backup_v1(&self, key: MegolmV1BackupKey) -> Result<(), CryptoStoreError> {
        if key.backup_version().is_some() {
            *self.backup_key.write().await = Some(key.clone());
            info!(backup_key =? key, "Activated a backup");
        } else {
            warn!(backup_key =? key, "Tried to activate a backup without having the backup key uploaded");
        }

        Ok(())
    }

    /// Get the number of backed up room keys and the total number of room keys.
    pub async fn room_key_counts(&self) -> Result<RoomKeyCounts, CryptoStoreError> {
        self.store.inbound_group_session_counts().await
    }

    /// Disable and reset our backup state.
    ///
    /// This will remove any pending backup request, remove the backup key and
    /// reset the backup state of each room key we have.
    #[instrument(skip(self))]
    pub async fn disable_backup(&self) -> Result<(), CryptoStoreError> {
        debug!("Disabling key backup and resetting backup state for room keys");

        self.backup_key.write().await.take();
        self.pending_backup.write().await.take();

        self.store.reset_backup_state().await?;

        debug!("Done disabling backup");

        Ok(())
    }

    /// Store the recovery key in the cryptostore.
    ///
    /// This is useful if the client wants to support gossiping of the backup
    /// key.
    pub async fn save_recovery_key(
        &self,
        recovery_key: Option<RecoveryKey>,
        version: Option<String>,
    ) -> Result<(), CryptoStoreError> {
        let changes = Changes { recovery_key, backup_version: version, ..Default::default() };
        self.store.save_changes(changes).await
    }

    /// Get the backup keys we have saved in our crypto store.
    pub async fn get_backup_keys(&self) -> Result<BackupKeys, CryptoStoreError> {
        self.store.load_backup_keys().await
    }

    /// Encrypt a batch of room keys and return a request that needs to be sent
    /// out to backup the room keys.
    pub async fn backup(&self) -> Result<Option<OutgoingRequest>, CryptoStoreError> {
        let mut request = self.pending_backup.write().await;

        if let Some(request) = &*request {
            trace!("Backing up, returning an existing request");

            Ok(Some(request.clone().into()))
        } else {
            trace!("Backing up, creating a new request");

            let new_request = self.backup_helper().await?;
            *request = new_request.clone();

            Ok(new_request.map(|r| r.into()))
        }
    }

    pub(crate) async fn mark_request_as_sent(
        &self,
        request_id: Uuid,
    ) -> Result<(), CryptoStoreError> {
        let mut request = self.pending_backup.write().await;

        if let Some(r) = &*request {
            if r.request_id == request_id {
                let sessions: Vec<_> = self
                    .store
                    .get_inbound_group_sessions()
                    .await?
                    .into_iter()
                    .filter(|s| r.session_was_part_of_the_backup(s))
                    .collect();

                for session in &sessions {
                    session.mark_as_backed_up()
                }

                trace!(request_id =? r.request_id, keys =? r.sessions, "Marking room keys as backed up");

                let changes = Changes { inbound_group_sessions: sessions, ..Default::default() };
                self.store.save_changes(changes).await?;

                let counts = self.store.inbound_group_session_counts().await?;

                trace!(
                    room_key_counts =? counts,
                    request_id =? r.request_id, keys =? r.sessions, "Marked room keys as backed up"
                );

                *request = None;
            } else {
                warn!(
                    expected = r.request_id.to_string().as_str(),
                    got = request_id.to_string().as_str(),
                    "Tried to mark a pending backup as sent but the request id didn't match"
                )
            }
        } else {
            warn!(
                request_id = request_id.to_string().as_str(),
                "Tried to mark a pending backup as sent but there isn't a backup pending"
            );
        }

        Ok(())
    }

    async fn backup_helper(&self) -> Result<Option<PendingBackup>, CryptoStoreError> {
        if let Some(backup_key) = &*self.backup_key.read().await {
            if let Some(version) = backup_key.backup_version() {
                let sessions =
                    self.store.inbound_group_sessions_for_backup(Self::BACKUP_BATCH_SIZE).await?;

                if !sessions.is_empty() {
                    let key_count = sessions.len();
                    let (backup, session_record) = Self::backup_keys(sessions, backup_key).await;

                    info!(
                        key_count = key_count,
                        keys =? session_record,
                        backup_key =? backup_key,
                        "Successfully created a room keys backup request"
                    );

                    let request = PendingBackup {
                        request_id: Uuid::new_v4(),
                        request: KeysBackupRequest { version, rooms: backup },
                        sessions: session_record,
                    };

                    Ok(Some(request))
                } else {
                    trace!(?backup_key, "No room keys need to be backed up");
                    Ok(None)
                }
            } else {
                warn!("Trying to backup room keys but the backup key wasn't uploaded");
                Ok(None)
            }
        } else {
            warn!("Trying to backup room keys but no backup key was found");
            Ok(None)
        }
    }

    /// Backup all the non-backed up room keys we know about
    async fn backup_keys(
        sessions: Vec<InboundGroupSession>,
        backup_key: &MegolmV1BackupKey,
    ) -> (
        BTreeMap<Box<RoomId>, RoomKeyBackup>,
        BTreeMap<Box<RoomId>, BTreeMap<String, BTreeSet<String>>>,
    ) {
        let mut backup: BTreeMap<Box<RoomId>, RoomKeyBackup> = BTreeMap::new();
        let mut session_record: BTreeMap<Box<RoomId>, BTreeMap<String, BTreeSet<String>>> =
            BTreeMap::new();

        for session in sessions {
            let room_id = session.room_id().to_owned();
            let session_id = session.session_id().to_owned();
            let sender_key = session.sender_key().to_owned();
            let session = backup_key.encrypt(session).await;

            session_record
                .entry(room_id.to_owned())
                .or_default()
                .entry(sender_key)
                .or_default()
                .insert(session_id.clone());

            let session = Raw::from_json(
                serde_json::value::to_raw_value(&session)
                    .expect("Can't serialize a backed up room key"),
            );

            backup
                .entry(room_id)
                .or_insert_with(|| RoomKeyBackup::new(BTreeMap::new()))
                .sessions
                .insert(session_id, session);
        }

        (backup, session_record)
    }
}

#[cfg(test)]
mod test {
    use matrix_sdk_test::async_test;
    use ruma::{device_id, room_id, user_id, DeviceId, RoomId, UserId};

    use super::RecoveryKey;
    use crate::{OlmError, OlmMachine};

    fn alice_id() -> &'static UserId {
        user_id!("@alice:example.org")
    }

    fn alice_device_id() -> &'static DeviceId {
        device_id!("JLAFKJWSCS")
    }

    fn room_id() -> &'static RoomId {
        room_id!("!test:localhost")
    }

    fn room_id2() -> &'static RoomId {
        room_id!("!test2:localhost")
    }

    async fn backup_flow(machine: OlmMachine) -> Result<(), OlmError> {
        let backup_machine = machine.backup_machine();
        let counts = backup_machine.store.inbound_group_session_counts().await?;

        assert_eq!(counts.total, 0, "Initially no keys exist");
        assert_eq!(counts.backed_up, 0, "Initially no backed up keys exist");

        machine.create_outbound_group_session_with_defaults(room_id()).await?;
        machine.create_outbound_group_session_with_defaults(room_id2()).await?;

        let counts = backup_machine.store.inbound_group_session_counts().await?;
        assert_eq!(counts.total, 2, "Two room keys need to exist in the store");
        assert_eq!(counts.backed_up, 0, "No room keys have been backed up yet");

        let recovery_key = RecoveryKey::new().expect("Can't create new recovery key");
        let backup_key = recovery_key.megolm_v1_public_key();
        backup_key.set_version("1".to_owned());

        backup_machine.enable_backup_v1(backup_key).await?;

        let request =
            backup_machine.backup().await?.expect("Created a backup request successfully");
        assert_eq!(
            Some(request.request_id),
            backup_machine.backup().await?.map(|r| r.request_id),
            "Calling backup again without uploading creates the same backup request"
        );

        backup_machine.mark_request_as_sent(request.request_id).await?;

        let counts = backup_machine.store.inbound_group_session_counts().await?;
        assert_eq!(counts.total, 2);
        assert_eq!(counts.backed_up, 2, "All room keys have been backed up");

        assert!(
            backup_machine.backup().await?.is_none(),
            "No room keys need to be backed up, no request needs to be created"
        );

        backup_machine.disable_backup().await?;

        let counts = backup_machine.store.inbound_group_session_counts().await?;
        assert_eq!(counts.total, 2);
        assert_eq!(
            counts.backed_up, 0,
            "Disabling the backup resets the backup flag on the room keys"
        );

        Ok(())
    }

    #[async_test]
    async fn memory_store_backups() -> Result<(), OlmError> {
        let machine = OlmMachine::new(alice_id(), alice_device_id());

        backup_flow(machine).await
    }

    #[async_test]
    #[cfg(feature = "sled_cryptostore")]
    async fn default_store_backups() -> Result<(), OlmError> {
        use tempfile::tempdir;

        let tmpdir = tempdir().expect("Can't create a temporary dir");
        let machine = OlmMachine::new_with_default_store(
            alice_id(),
            alice_device_id(),
            tmpdir.as_ref(),
            None,
        )
        .await?;

        backup_flow(machine).await
    }

    #[async_test]
    #[cfg(feature = "sled_cryptostore")]
    async fn recovery_key_storing() -> Result<(), OlmError> {
        use tempfile::tempdir;

        let tmpdir = tempdir().expect("Can't create a temporary dir");
        let machine = OlmMachine::new_with_default_store(
            alice_id(),
            alice_device_id(),
            tmpdir.as_ref(),
            Some("test"),
        )
        .await?;
        let backup_machine = machine.backup_machine();

        let recovery_key = RecoveryKey::new().expect("Can't create new recovery key");
        let encoded_key = recovery_key.to_base64();

        backup_machine.save_recovery_key(Some(recovery_key), Some("1".to_owned())).await?;

        let loded_backup = backup_machine.get_backup_keys().await?;

        assert_eq!(
            encoded_key,
            loded_backup
                .recovery_key
                .expect("The recovery key wasn't loaded from the store")
                .to_base64(),
            "The loaded key matches to the one we stored"
        );

        assert_eq!(
            Some("1"),
            loded_backup.backup_version.as_deref(),
            "The loaded version matches to the one we stored"
        );

        Ok(())
    }
}
