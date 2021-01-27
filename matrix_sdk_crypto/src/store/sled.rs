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

use std::{
    collections::{HashMap, HashSet},
    convert::TryFrom,
    path::Path,
    sync::Arc,
};

use dashmap::DashSet;
use olm_rs::PicklingMode;
pub use sled::Error;
use sled::{
    transaction::{ConflictableTransactionError, TransactionError},
    Config, Db, Transactional, Tree,
};

use matrix_sdk_common::{
    async_trait,
    identifiers::{DeviceId, DeviceIdBox, RoomId, UserId},
    locks::Mutex,
};

use super::{
    caches::SessionStore, Changes, CryptoStore, CryptoStoreError, InboundGroupSession, PickleKey,
    ReadOnlyAccount, Result, Session,
};
use crate::{
    identities::{ReadOnlyDevice, UserIdentities},
    olm::{OutboundGroupSession, PickledInboundGroupSession, PrivateCrossSigningIdentity},
};

/// This needs to be 32 bytes long since AES-GCM requires it, otherwise we will
/// panic once we try to pickle a Signing object.
const DEFAULT_PICKLE: &str = "DEFAULT_PICKLE_PASSPHRASE_123456";

trait EncodeKey {
    const SEPARATOR: u8 = 0xff;
    fn encode(&self) -> Vec<u8>;
}

impl EncodeKey for &UserId {
    fn encode(&self) -> Vec<u8> {
        self.as_str().encode()
    }
}

impl EncodeKey for &RoomId {
    fn encode(&self) -> Vec<u8> {
        self.as_str().encode()
    }
}

impl EncodeKey for &str {
    fn encode(&self) -> Vec<u8> {
        [self.as_bytes(), &[Self::SEPARATOR]].concat()
    }
}

impl EncodeKey for (&str, &str) {
    fn encode(&self) -> Vec<u8> {
        [
            self.0.as_bytes(),
            &[Self::SEPARATOR],
            self.1.as_bytes(),
            &[Self::SEPARATOR],
        ]
        .concat()
    }
}

impl EncodeKey for (&str, &str, &str) {
    fn encode(&self) -> Vec<u8> {
        [
            self.0.as_bytes(),
            &[Self::SEPARATOR],
            self.1.as_bytes(),
            &[Self::SEPARATOR],
            self.2.as_bytes(),
            &[Self::SEPARATOR],
        ]
        .concat()
    }
}

/// An in-memory only store that will forget all the E2EE key once it's dropped.
#[derive(Debug, Clone)]
pub struct SledStore {
    inner: Db,
    pickle_key: Arc<PickleKey>,

    session_cache: SessionStore,
    tracked_users_cache: Arc<DashSet<UserId>>,
    users_for_key_query_cache: Arc<DashSet<UserId>>,

    account: Tree,
    private_identity: Tree,

    olm_hashes: Tree,
    sessions: Tree,
    inbound_group_sessions: Tree,
    outbound_group_sessions: Tree,

    devices: Tree,
    identities: Tree,

    tracked_users: Tree,
    users_for_key_query: Tree,
    values: Tree,
}

impl From<TransactionError<serde_json::Error>> for CryptoStoreError {
    fn from(e: TransactionError<serde_json::Error>) -> Self {
        match e {
            TransactionError::Abort(e) => CryptoStoreError::Serialization(e),
            TransactionError::Storage(e) => CryptoStoreError::Database(e),
        }
    }
}

impl SledStore {
    /// Open the sled based cryptostore at the given path using the given
    /// passphrase to encrypt private data.
    pub fn open_with_passphrase(path: impl AsRef<Path>, passphrase: Option<&str>) -> Result<Self> {
        let path = path.as_ref().join("matrix-sdk-crypto");
        let db = Config::new().temporary(false).path(path).open()?;

        SledStore::open_helper(db, passphrase)
    }

    /// Create a sled based cryptostore using the given sled database.
    /// The given passphrase will be used to encrypt private data.
    pub fn open_with_database(db: Db, passphrase: Option<&str>) -> Result<Self> {
        SledStore::open_helper(db, passphrase)
    }

    fn open_helper(db: Db, passphrase: Option<&str>) -> Result<Self> {
        let account = db.open_tree("account")?;
        let private_identity = db.open_tree("private_identity")?;

        let sessions = db.open_tree("session")?;
        let inbound_group_sessions = db.open_tree("inbound_group_sessions")?;
        let outbound_group_sessions = db.open_tree("outbound_group_sessions")?;
        let tracked_users = db.open_tree("tracked_users")?;
        let users_for_key_query = db.open_tree("users_for_key_query")?;
        let olm_hashes = db.open_tree("olm_hashes")?;

        let devices = db.open_tree("devices")?;
        let identities = db.open_tree("identities")?;
        let values = db.open_tree("values")?;

        let session_cache = SessionStore::new();

        let pickle_key = if let Some(passphrase) = passphrase {
            Self::get_or_create_pickle_key(&passphrase, &db)?
        } else {
            PickleKey::try_from(DEFAULT_PICKLE.as_bytes().to_vec())
                .expect("Can't create default pickle key")
        };

        Ok(Self {
            inner: db,
            pickle_key: pickle_key.into(),
            account,
            private_identity,
            sessions,
            session_cache,
            tracked_users_cache: DashSet::new().into(),
            users_for_key_query_cache: DashSet::new().into(),
            inbound_group_sessions,
            outbound_group_sessions,
            devices,
            tracked_users,
            users_for_key_query,
            olm_hashes,
            identities,
            values,
        })
    }

    fn get_or_create_pickle_key(passphrase: &str, database: &Db) -> Result<PickleKey> {
        let key = if let Some(key) = database
            .get("pickle_key".encode())?
            .map(|v| serde_json::from_slice(&v))
        {
            PickleKey::from_encrypted(passphrase, key?)
                .map_err(|_| CryptoStoreError::UnpicklingError)?
        } else {
            let key = PickleKey::new();
            let encrypted = key.encrypt(passphrase);
            database.insert("pickle_key".encode(), serde_json::to_vec(&encrypted)?)?;
            key
        };

        Ok(key)
    }

    fn get_pickle_mode(&self) -> PicklingMode {
        self.pickle_key.pickle_mode()
    }

    fn get_pickle_key(&self) -> &[u8] {
        self.pickle_key.key()
    }

    async fn load_tracked_users(&self) -> Result<()> {
        for value in self.tracked_users.iter() {
            let (user, dirty) = value?;
            let user = UserId::try_from(String::from_utf8_lossy(&user).to_string())?;
            let dirty = dirty.get(0).map(|d| *d == 1).unwrap_or(true);

            self.tracked_users_cache.insert(user.clone());

            if dirty {
                self.users_for_key_query_cache.insert(user);
            }
        }

        Ok(())
    }

    async fn load_outbound_group_session(
        &self,
        room_id: &RoomId,
    ) -> Result<Option<OutboundGroupSession>> {
        let account = self
            .load_account()
            .await?
            .ok_or(CryptoStoreError::AccountUnset)?;

        let device_id: Arc<DeviceIdBox> = account.device_id().to_owned().into();
        let identity_keys = account.identity_keys;

        self.outbound_group_sessions
            .get(room_id.encode())?
            .map(|p| serde_json::from_slice(&p).map_err(CryptoStoreError::Serialization))
            .transpose()?
            .map(|p| {
                OutboundGroupSession::from_pickle(
                    device_id,
                    identity_keys,
                    p,
                    self.get_pickle_mode(),
                )
                .map_err(CryptoStoreError::OlmGroupSession)
            })
            .transpose()
    }

    async fn save_changes(&self, changes: Changes) -> Result<()> {
        let account_pickle = if let Some(a) = changes.account {
            Some(a.pickle(self.get_pickle_mode()).await)
        } else {
            None
        };

        let private_identity_pickle = if let Some(i) = changes.private_identity {
            Some(i.pickle(DEFAULT_PICKLE.as_bytes()).await?)
        } else {
            None
        };

        let device_changes = changes.devices;
        let mut session_changes = HashMap::new();

        for session in changes.sessions {
            let sender_key = session.sender_key();
            let session_id = session.session_id();

            let pickle = session.pickle(self.get_pickle_mode()).await;
            let key = (sender_key, session_id).encode();

            self.session_cache.add(session).await;
            session_changes.insert(key, pickle);
        }

        let mut inbound_session_changes = HashMap::new();

        for session in changes.inbound_group_sessions {
            let room_id = session.room_id();
            let sender_key = session.sender_key();
            let session_id = session.session_id();
            let key = (room_id.as_str(), sender_key, session_id).encode();
            let pickle = session.pickle(self.get_pickle_mode()).await;

            inbound_session_changes.insert(key, pickle);
        }

        let mut outbound_session_changes = HashMap::new();

        for session in changes.outbound_group_sessions {
            let room_id = session.room_id();
            let pickle = session.pickle(self.get_pickle_mode()).await;

            outbound_session_changes.insert(room_id.clone(), pickle);
        }

        let identity_changes = changes.identities;
        let olm_hashes = changes.message_hashes;

        let ret: std::result::Result<(), TransactionError<serde_json::Error>> = (
            &self.account,
            &self.private_identity,
            &self.devices,
            &self.identities,
            &self.sessions,
            &self.inbound_group_sessions,
            &self.outbound_group_sessions,
            &self.olm_hashes,
        )
            .transaction(
                |(
                    account,
                    private_identity,
                    devices,
                    identities,
                    sessions,
                    inbound_sessions,
                    outbound_sessions,
                    hashes,
                )| {
                    if let Some(a) = &account_pickle {
                        account.insert(
                            "account".encode(),
                            serde_json::to_vec(a).map_err(ConflictableTransactionError::Abort)?,
                        )?;
                    }

                    if let Some(i) = &private_identity_pickle {
                        private_identity.insert(
                            "identity".encode(),
                            serde_json::to_vec(&i).map_err(ConflictableTransactionError::Abort)?,
                        )?;
                    }

                    for device in device_changes.new.iter().chain(&device_changes.changed) {
                        let key = (device.user_id().as_str(), device.device_id().as_str()).encode();
                        let device = serde_json::to_vec(&device)
                            .map_err(ConflictableTransactionError::Abort)?;
                        devices.insert(key, device)?;
                    }

                    for device in &device_changes.deleted {
                        let key = (device.user_id().as_str(), device.device_id().as_str()).encode();
                        devices.remove(key)?;
                    }

                    for identity in identity_changes.changed.iter().chain(&identity_changes.new) {
                        identities.insert(
                            identity.user_id().encode(),
                            serde_json::to_vec(&identity)
                                .map_err(ConflictableTransactionError::Abort)?,
                        )?;
                    }

                    for (key, session) in &session_changes {
                        sessions.insert(
                            key.as_slice(),
                            serde_json::to_vec(&session)
                                .map_err(ConflictableTransactionError::Abort)?,
                        )?;
                    }

                    for (key, session) in &inbound_session_changes {
                        inbound_sessions.insert(
                            key.as_slice(),
                            serde_json::to_vec(&session)
                                .map_err(ConflictableTransactionError::Abort)?,
                        )?;
                    }

                    for (key, session) in &outbound_session_changes {
                        outbound_sessions.insert(
                            key.encode(),
                            serde_json::to_vec(&session)
                                .map_err(ConflictableTransactionError::Abort)?,
                        )?;
                    }

                    for hash in &olm_hashes {
                        hashes.insert(
                            serde_json::to_vec(&hash)
                                .map_err(ConflictableTransactionError::Abort)?,
                            &[0],
                        )?;
                    }

                    Ok(())
                },
            );

        ret?;
        self.inner.flush_async().await?;

        Ok(())
    }
}

#[async_trait]
impl CryptoStore for SledStore {
    async fn load_account(&self) -> Result<Option<ReadOnlyAccount>> {
        if let Some(pickle) = self.account.get("account".encode())? {
            let pickle = serde_json::from_slice(&pickle)?;

            self.load_tracked_users().await?;

            Ok(Some(ReadOnlyAccount::from_pickle(
                pickle,
                self.get_pickle_mode(),
            )?))
        } else {
            Ok(None)
        }
    }

    async fn save_account(&self, account: ReadOnlyAccount) -> Result<()> {
        let pickle = account.pickle(self.get_pickle_mode()).await;
        self.account
            .insert("account".encode(), serde_json::to_vec(&pickle)?)?;

        Ok(())
    }

    async fn save_changes(&self, changes: Changes) -> Result<()> {
        self.save_changes(changes).await
    }

    async fn get_sessions(&self, sender_key: &str) -> Result<Option<Arc<Mutex<Vec<Session>>>>> {
        let account = self
            .load_account()
            .await?
            .ok_or(CryptoStoreError::AccountUnset)?;

        if self.session_cache.get(sender_key).is_none() {
            let sessions: Result<Vec<Session>> = self
                .sessions
                .scan_prefix(sender_key.encode())
                .map(|s| serde_json::from_slice(&s?.1).map_err(CryptoStoreError::Serialization))
                .map(|p| {
                    Session::from_pickle(
                        account.user_id.clone(),
                        account.device_id.clone(),
                        account.identity_keys.clone(),
                        p?,
                        self.get_pickle_mode(),
                    )
                    .map_err(CryptoStoreError::SessionUnpickling)
                })
                .collect();

            self.session_cache.set_for_sender(sender_key, sessions?);
        }

        Ok(self.session_cache.get(sender_key))
    }

    async fn get_inbound_group_session(
        &self,
        room_id: &RoomId,
        sender_key: &str,
        session_id: &str,
    ) -> Result<Option<InboundGroupSession>> {
        let key = (room_id.as_str(), sender_key, session_id).encode();
        let pickle = self
            .inbound_group_sessions
            .get(&key)?
            .map(|p| serde_json::from_slice(&p));

        if let Some(pickle) = pickle {
            Ok(Some(InboundGroupSession::from_pickle(
                pickle?,
                self.get_pickle_mode(),
            )?))
        } else {
            Ok(None)
        }
    }

    async fn get_inbound_group_sessions(&self) -> Result<Vec<InboundGroupSession>> {
        let pickles: Result<Vec<PickledInboundGroupSession>> = self
            .inbound_group_sessions
            .iter()
            .map(|p| serde_json::from_slice(&p?.1).map_err(CryptoStoreError::Serialization))
            .collect();

        Ok(pickles?
            .into_iter()
            .filter_map(|p| InboundGroupSession::from_pickle(p, self.get_pickle_mode()).ok())
            .collect())
    }

    fn users_for_key_query(&self) -> HashSet<UserId> {
        #[allow(clippy::map_clone)]
        self.users_for_key_query_cache
            .iter()
            .map(|u| u.clone())
            .collect()
    }

    fn is_user_tracked(&self, user_id: &UserId) -> bool {
        self.tracked_users_cache.contains(user_id)
    }

    fn has_users_for_key_query(&self) -> bool {
        !self.users_for_key_query_cache.is_empty()
    }

    async fn update_tracked_user(&self, user: &UserId, dirty: bool) -> Result<bool> {
        let already_added = self.tracked_users_cache.insert(user.clone());

        if dirty {
            self.users_for_key_query_cache.insert(user.clone());
        } else {
            self.users_for_key_query_cache.remove(user);
        }

        self.tracked_users.insert(user.as_str(), &[dirty as u8])?;

        Ok(already_added)
    }

    async fn get_device(
        &self,
        user_id: &UserId,
        device_id: &DeviceId,
    ) -> Result<Option<ReadOnlyDevice>> {
        let key = (user_id.as_str(), device_id.as_str()).encode();

        if let Some(d) = self.devices.get(key)? {
            Ok(Some(serde_json::from_slice(&d)?))
        } else {
            Ok(None)
        }
    }

    async fn get_user_devices(
        &self,
        user_id: &UserId,
    ) -> Result<HashMap<DeviceIdBox, ReadOnlyDevice>> {
        self.devices
            .scan_prefix(user_id.encode())
            .map(|d| serde_json::from_slice(&d?.1).map_err(CryptoStoreError::Serialization))
            .map(|d| {
                let d: ReadOnlyDevice = d?;
                Ok((d.device_id().to_owned(), d))
            })
            .collect()
    }

    async fn get_user_identity(&self, user_id: &UserId) -> Result<Option<UserIdentities>> {
        Ok(self
            .identities
            .get(user_id.encode())?
            .map(|i| serde_json::from_slice(&i))
            .transpose()?)
    }

    async fn save_value(&self, key: String, value: String) -> Result<()> {
        self.values.insert(key.as_str().encode(), value.as_str())?;
        Ok(())
    }

    async fn remove_value(&self, key: &str) -> Result<()> {
        self.values.remove(key.encode())?;
        Ok(())
    }

    async fn get_value(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .values
            .get(key.encode())?
            .map(|v| String::from_utf8_lossy(&v).to_string()))
    }

    async fn load_identity(&self) -> Result<Option<PrivateCrossSigningIdentity>> {
        if let Some(i) = self.private_identity.get("identity".encode())? {
            let pickle = serde_json::from_slice(&i)?;
            Ok(Some(
                PrivateCrossSigningIdentity::from_pickle(pickle, self.get_pickle_key())
                    .await
                    .map_err(|_| CryptoStoreError::UnpicklingError)?,
            ))
        } else {
            Ok(None)
        }
    }

    async fn is_message_known(&self, message_hash: &crate::olm::OlmMessageHash) -> Result<bool> {
        Ok(self
            .olm_hashes
            .contains_key(serde_json::to_vec(message_hash)?)?)
    }

    async fn get_outbound_group_sessions(
        &self,
        room_id: &RoomId,
    ) -> Result<Option<OutboundGroupSession>> {
        self.load_outbound_group_session(room_id).await
    }
}

#[cfg(test)]
mod test {
    use crate::{
        identities::{
            device::test::get_device,
            user::test::{get_other_identity, get_own_identity},
        },
        olm::{
            GroupSessionKey, InboundGroupSession, OlmMessageHash, PrivateCrossSigningIdentity,
            ReadOnlyAccount, Session,
        },
        store::{Changes, DeviceChanges, IdentityChanges},
    };
    use matrix_sdk_common::{
        api::r0::keys::SignedKey,
        identifiers::{room_id, user_id, DeviceId, UserId},
    };
    use matrix_sdk_test::async_test;
    use olm_rs::outbound_group_session::OlmOutboundGroupSession;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    use super::{CryptoStore, SledStore};

    fn alice_id() -> UserId {
        user_id!("@alice:example.org")
    }

    fn alice_device_id() -> Box<DeviceId> {
        "ALICEDEVICE".into()
    }

    fn bob_id() -> UserId {
        user_id!("@bob:example.org")
    }

    fn bob_device_id() -> Box<DeviceId> {
        "BOBDEVICE".into()
    }

    async fn get_store(passphrase: Option<&str>) -> (SledStore, tempfile::TempDir) {
        let tmpdir = tempdir().unwrap();
        let tmpdir_path = tmpdir.path().to_str().unwrap();

        let store = SledStore::open_with_passphrase(tmpdir_path, passphrase)
            .expect("Can't create a passphrase protected store");

        (store, tmpdir)
    }

    async fn get_loaded_store() -> (ReadOnlyAccount, SledStore, tempfile::TempDir) {
        let (store, dir) = get_store(None).await;
        let account = get_account();
        store
            .save_account(account.clone())
            .await
            .expect("Can't save account");

        (account, store, dir)
    }

    fn get_account() -> ReadOnlyAccount {
        ReadOnlyAccount::new(&alice_id(), &alice_device_id())
    }

    async fn get_account_and_session() -> (ReadOnlyAccount, Session) {
        let alice = ReadOnlyAccount::new(&alice_id(), &alice_device_id());
        let bob = ReadOnlyAccount::new(&bob_id(), &bob_device_id());

        bob.generate_one_time_keys_helper(1).await;
        let one_time_key = bob
            .one_time_keys()
            .await
            .curve25519()
            .iter()
            .next()
            .unwrap()
            .1
            .to_owned();
        let one_time_key = SignedKey {
            key: one_time_key,
            signatures: BTreeMap::new(),
        };
        let sender_key = bob.identity_keys().curve25519().to_owned();
        let session = alice
            .create_outbound_session_helper(&sender_key, &one_time_key)
            .await
            .unwrap();

        (alice, session)
    }

    #[async_test]
    async fn create_store() {
        let tmpdir = tempdir().unwrap();
        let tmpdir_path = tmpdir.path().to_str().unwrap();
        let _ = SledStore::open_with_passphrase(tmpdir_path, None).expect("Can't create store");
    }

    #[async_test]
    async fn save_account() {
        let (store, _dir) = get_store(None).await;
        assert!(store.load_account().await.unwrap().is_none());
        let account = get_account();

        store
            .save_account(account)
            .await
            .expect("Can't save account");
    }

    #[async_test]
    async fn load_account() {
        let (store, _dir) = get_store(None).await;
        let account = get_account();

        store
            .save_account(account.clone())
            .await
            .expect("Can't save account");

        let loaded_account = store.load_account().await.expect("Can't load account");
        let loaded_account = loaded_account.unwrap();

        assert_eq!(account, loaded_account);
    }

    #[async_test]
    async fn load_account_with_passphrase() {
        let (store, _dir) = get_store(Some("secret_passphrase")).await;
        let account = get_account();

        store
            .save_account(account.clone())
            .await
            .expect("Can't save account");

        let loaded_account = store.load_account().await.expect("Can't load account");
        let loaded_account = loaded_account.unwrap();

        assert_eq!(account, loaded_account);
    }

    #[async_test]
    async fn save_and_share_account() {
        let (store, _dir) = get_store(None).await;
        let account = get_account();

        store
            .save_account(account.clone())
            .await
            .expect("Can't save account");

        account.mark_as_shared();
        account.update_uploaded_key_count(50);

        store
            .save_account(account.clone())
            .await
            .expect("Can't save account");

        let loaded_account = store.load_account().await.expect("Can't load account");
        let loaded_account = loaded_account.unwrap();

        assert_eq!(account, loaded_account);
        assert_eq!(
            account.uploaded_key_count(),
            loaded_account.uploaded_key_count()
        );
    }

    #[async_test]
    async fn load_sessions() {
        let (store, _dir) = get_store(None).await;
        let (account, session) = get_account_and_session().await;
        store
            .save_account(account.clone())
            .await
            .expect("Can't save account");

        let changes = Changes {
            sessions: vec![session.clone()],
            ..Default::default()
        };

        store.save_changes(changes).await.unwrap();

        let sessions = store
            .get_sessions(&session.sender_key)
            .await
            .expect("Can't load sessions")
            .unwrap();
        let loaded_session = sessions.lock().await.get(0).cloned().unwrap();

        assert_eq!(&session, &loaded_session);
    }

    #[async_test]
    async fn add_and_save_session() {
        let (store, dir) = get_store(None).await;
        let (account, session) = get_account_and_session().await;
        let sender_key = session.sender_key.to_owned();
        let session_id = session.session_id().to_owned();

        store
            .save_account(account.clone())
            .await
            .expect("Can't save account");

        let changes = Changes {
            sessions: vec![session.clone()],
            ..Default::default()
        };
        store.save_changes(changes).await.unwrap();

        let sessions = store.get_sessions(&sender_key).await.unwrap().unwrap();
        let sessions_lock = sessions.lock().await;
        let session = &sessions_lock[0];

        assert_eq!(session_id, session.session_id());

        drop(store);

        let store = SledStore::open_with_passphrase(dir.path(), None).expect("Can't create store");

        let loaded_account = store.load_account().await.unwrap().unwrap();
        assert_eq!(account, loaded_account);

        let sessions = store.get_sessions(&sender_key).await.unwrap().unwrap();
        let sessions_lock = sessions.lock().await;
        let session = &sessions_lock[0];

        assert_eq!(session_id, session.session_id());
    }

    #[async_test]
    async fn save_inbound_group_session() {
        let (account, store, _dir) = get_loaded_store().await;

        let identity_keys = account.identity_keys();
        let outbound_session = OlmOutboundGroupSession::new();
        let session = InboundGroupSession::new(
            identity_keys.curve25519(),
            identity_keys.ed25519(),
            &room_id!("!test:localhost"),
            GroupSessionKey(outbound_session.session_key()),
        )
        .expect("Can't create session");

        let changes = Changes {
            inbound_group_sessions: vec![session],
            ..Default::default()
        };

        store
            .save_changes(changes)
            .await
            .expect("Can't save group session");
    }

    #[async_test]
    async fn load_inbound_group_session() {
        let (account, store, dir) = get_loaded_store().await;

        let identity_keys = account.identity_keys();
        let outbound_session = OlmOutboundGroupSession::new();
        let session = InboundGroupSession::new(
            identity_keys.curve25519(),
            identity_keys.ed25519(),
            &room_id!("!test:localhost"),
            GroupSessionKey(outbound_session.session_key()),
        )
        .expect("Can't create session");

        let mut export = session.export().await;

        export.forwarding_curve25519_key_chain = vec!["some_chain".to_owned()];

        let session = InboundGroupSession::from_export(export).unwrap();

        let changes = Changes {
            inbound_group_sessions: vec![session.clone()],
            ..Default::default()
        };

        store
            .save_changes(changes)
            .await
            .expect("Can't save group session");

        drop(store);

        let store = SledStore::open_with_passphrase(dir.path(), None).expect("Can't create store");

        store.load_account().await.unwrap();

        let loaded_session = store
            .get_inbound_group_session(&session.room_id, &session.sender_key, session.session_id())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session, loaded_session);
        let export = loaded_session.export().await;
        assert!(!export.forwarding_curve25519_key_chain.is_empty())
    }

    #[async_test]
    async fn test_tracked_users() {
        let (_account, store, dir) = get_loaded_store().await;
        let device = get_device();

        assert!(store
            .update_tracked_user(device.user_id(), false)
            .await
            .unwrap());
        assert!(!store
            .update_tracked_user(device.user_id(), false)
            .await
            .unwrap());

        assert!(store.is_user_tracked(device.user_id()));
        assert!(!store.users_for_key_query().contains(device.user_id()));
        assert!(!store
            .update_tracked_user(device.user_id(), true)
            .await
            .unwrap());
        assert!(store.users_for_key_query().contains(device.user_id()));
        drop(store);

        let store = SledStore::open_with_passphrase(dir.path(), None).expect("Can't create store");

        store.load_account().await.unwrap();

        assert!(store.is_user_tracked(device.user_id()));
        assert!(store.users_for_key_query().contains(device.user_id()));

        store
            .update_tracked_user(device.user_id(), false)
            .await
            .unwrap();
        assert!(!store.users_for_key_query().contains(device.user_id()));
        drop(store);

        let store = SledStore::open_with_passphrase(dir.path(), None).expect("Can't create store");

        store.load_account().await.unwrap();

        assert!(!store.users_for_key_query().contains(device.user_id()));
    }

    #[async_test]
    async fn device_saving() {
        let (_account, store, dir) = get_loaded_store().await;
        let device = get_device();

        let changes = Changes {
            devices: DeviceChanges {
                changed: vec![device.clone()],
                ..Default::default()
            },
            ..Default::default()
        };

        store.save_changes(changes).await.unwrap();

        drop(store);

        let store = SledStore::open_with_passphrase(dir.path(), None).expect("Can't create store");

        store.load_account().await.unwrap();

        let loaded_device = store
            .get_device(device.user_id(), device.device_id())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(device, loaded_device);

        for algorithm in loaded_device.algorithms() {
            assert!(device.algorithms().contains(algorithm));
        }
        assert_eq!(device.algorithms().len(), loaded_device.algorithms().len());
        assert_eq!(device.keys(), loaded_device.keys());

        let user_devices = store.get_user_devices(device.user_id()).await.unwrap();
        assert_eq!(&**user_devices.keys().next().unwrap(), device.device_id());
        assert_eq!(user_devices.values().next().unwrap(), &device);
    }

    #[async_test]
    async fn device_deleting() {
        let (_account, store, dir) = get_loaded_store().await;
        let device = get_device();

        let changes = Changes {
            devices: DeviceChanges {
                changed: vec![device.clone()],
                ..Default::default()
            },
            ..Default::default()
        };

        store.save_changes(changes).await.unwrap();

        let changes = Changes {
            devices: DeviceChanges {
                deleted: vec![device.clone()],
                ..Default::default()
            },
            ..Default::default()
        };

        store.save_changes(changes).await.unwrap();
        drop(store);

        let store = SledStore::open_with_passphrase(dir.path(), None).expect("Can't create store");

        store.load_account().await.unwrap();

        let loaded_device = store
            .get_device(device.user_id(), device.device_id())
            .await
            .unwrap();

        assert!(loaded_device.is_none());
    }

    #[async_test]
    async fn user_saving() {
        let dir = tempdir().unwrap();
        let tmpdir_path = dir.path().to_str().unwrap();

        let user_id = user_id!("@example:localhost");
        let device_id: &DeviceId = "WSKKLTJZCL".into();

        let store = SledStore::open_with_passphrase(tmpdir_path, None).expect("Can't create store");

        let account = ReadOnlyAccount::new(&user_id, &device_id);

        store
            .save_account(account.clone())
            .await
            .expect("Can't save account");

        let own_identity = get_own_identity();

        let changes = Changes {
            identities: IdentityChanges {
                changed: vec![own_identity.clone().into()],
                ..Default::default()
            },
            ..Default::default()
        };

        store
            .save_changes(changes)
            .await
            .expect("Can't save identity");

        drop(store);

        let store = SledStore::open_with_passphrase(dir.path(), None).expect("Can't create store");

        store.load_account().await.unwrap();

        let loaded_user = store
            .get_user_identity(own_identity.user_id())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(loaded_user.master_key(), own_identity.master_key());
        assert_eq!(
            loaded_user.self_signing_key(),
            own_identity.self_signing_key()
        );
        assert_eq!(loaded_user, own_identity.clone().into());

        let other_identity = get_other_identity();

        let changes = Changes {
            identities: IdentityChanges {
                changed: vec![other_identity.clone().into()],
                ..Default::default()
            },
            ..Default::default()
        };

        store.save_changes(changes).await.unwrap();

        let loaded_user = store
            .get_user_identity(other_identity.user_id())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(loaded_user.master_key(), other_identity.master_key());
        assert_eq!(
            loaded_user.self_signing_key(),
            other_identity.self_signing_key()
        );
        assert_eq!(loaded_user, other_identity.into());

        own_identity.mark_as_verified();

        let changes = Changes {
            identities: IdentityChanges {
                changed: vec![own_identity.into()],
                ..Default::default()
            },
            ..Default::default()
        };

        store.save_changes(changes).await.unwrap();
        let loaded_user = store.get_user_identity(&user_id).await.unwrap().unwrap();
        assert!(loaded_user.own().unwrap().is_verified())
    }

    #[async_test]
    async fn private_identity_saving() {
        let (_, store, _dir) = get_loaded_store().await;
        assert!(store.load_identity().await.unwrap().is_none());
        let identity = PrivateCrossSigningIdentity::new(alice_id()).await;

        let changes = Changes {
            private_identity: Some(identity.clone()),
            ..Default::default()
        };

        store.save_changes(changes).await.unwrap();
        let loaded_identity = store.load_identity().await.unwrap().unwrap();
        assert_eq!(identity.user_id(), loaded_identity.user_id());
    }

    #[async_test]
    async fn key_value_saving() {
        let (_, store, _dir) = get_loaded_store().await;
        let key = "test_key".to_string();
        let value = "secret value".to_string();

        store.save_value(key.clone(), value.clone()).await.unwrap();
        let stored_value = store.get_value(&key).await.unwrap().unwrap();

        assert_eq!(value, stored_value);

        store.remove_value(&key).await.unwrap();
        assert!(store.get_value(&key).await.unwrap().is_none());
    }

    #[async_test]
    async fn olm_hash_saving() {
        let (_, store, _dir) = get_loaded_store().await;

        let hash = OlmMessageHash {
            sender_key: "test_sender".to_owned(),
            hash: "test_hash".to_owned(),
        };

        let mut changes = Changes::default();
        changes.message_hashes.push(hash.clone());

        assert!(!store.is_message_known(&hash).await.unwrap());
        store.save_changes(changes).await.unwrap();
        assert!(store.is_message_known(&hash).await.unwrap());
    }
}
