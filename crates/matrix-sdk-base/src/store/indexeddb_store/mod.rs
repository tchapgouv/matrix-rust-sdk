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

mod store_key;
use indexed_db_futures::prelude::*;
use indexed_db_futures::js_sys::JsValue;

// use std::{
//     collections::BTreeSet,
//     convert::{TryFrom, TryInto},
//     path::{Path, PathBuf},
//     sync::Arc,
//     time::Instant,
// };

use futures_core::stream::Stream;
use futures_util::stream::{self, TryStreamExt};
use matrix_sdk_common::async_trait;
use ruma::{
    events::{
        presence::PresenceEvent,
        receipt::Receipt,
        room::member::{MembershipState, RoomMemberEventContent},
        AnyGlobalAccountDataEvent, AnyRoomAccountDataEvent, AnySyncStateEvent, EventType,
    },
    receipt::ReceiptType,
    serde::Raw,
    EventId, MxcUri, RoomId, UserId,
};
use serde::{Deserialize, Serialize};
// use sled::{
//     transaction::{ConflictableTransactionError, TransactionError},
//     Config, Db, Transactional, Tree,
// };
//use tokio::task::spawn_blocking;
use tracing::info;

use self::store_key::{EncryptedEvent, StoreKey};
use super::{Result, RoomInfo, StateChanges, StateStore, StoreError};
use crate::{
    deserialized_responses::MemberEvent,
    media::{MediaRequest, UniqueKey},
};

#[derive(Debug, Serialize, Deserialize)]
pub enum DatabaseType {
    Unencrypted,
    Encrypted(store_key::EncryptedStoreKey),
}

#[derive(Debug, thiserror::Error)]
pub enum SerializationError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Encryption(#[from] store_key::Error),
}

// impl From<TransactionError<SerializationError>> for StoreError {
//     fn from(e: TransactionError<SerializationError>) -> Self {
//         match e {
//             TransactionError::Abort(e) => e.into(),
//             TransactionError::Storage(e) => StoreError::Indexeddb(e),
//         }
//     }
// }

impl From<SerializationError> for StoreError {
    fn from(e: SerializationError) -> Self {
        match e {
            SerializationError::Json(e) => StoreError::Json(e),
            SerializationError::Encryption(e) => match e {
                store_key::Error::Random(e) => StoreError::Encryption(e.to_string()),
                store_key::Error::Serialization(e) => StoreError::Json(e),
                store_key::Error::Encryption(e) => StoreError::Encryption(e),
            },
        }
    }
}

const ENCODE_SEPARATOR: u8 = 0xff;

trait EncodeKey {
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
        [self.as_bytes(), &[ENCODE_SEPARATOR]].concat()
    }
}

impl EncodeKey for (&str, &str) {
    fn encode(&self) -> Vec<u8> {
        [self.0.as_bytes(), &[ENCODE_SEPARATOR], self.1.as_bytes(), &[ENCODE_SEPARATOR]].concat()
    }
}

impl EncodeKey for (&str, &str, &str) {
    fn encode(&self) -> Vec<u8> {
        [
            self.0.as_bytes(),
            &[ENCODE_SEPARATOR],
            self.1.as_bytes(),
            &[ENCODE_SEPARATOR],
            self.2.as_bytes(),
            &[ENCODE_SEPARATOR],
        ]
        .concat()
    }
}

impl EncodeKey for (&str, &str, &str, &str) {
    fn encode(&self) -> Vec<u8> {
        [
            self.0.as_bytes(),
            &[ENCODE_SEPARATOR],
            self.1.as_bytes(),
            &[ENCODE_SEPARATOR],
            self.2.as_bytes(),
            &[ENCODE_SEPARATOR],
            self.3.as_bytes(),
            &[ENCODE_SEPARATOR],
        ]
        .concat()
    }
}

impl EncodeKey for EventType {
    fn encode(&self) -> Vec<u8> {
        self.as_str().encode()
    }
}

/// Get the value at `position` in encoded `key`.
///
/// The key must have been encoded with the `EncodeKey` trait. `position`
/// corresponds to the position in the tuple before the key was encoded. If it
/// wasn't encoded in a tuple, use `0`.
///
/// Returns `None` if there is no key at `position`.
pub fn decode_key_value(key: &[u8], position: usize) -> Option<String> {
    let values: Vec<&[u8]> = key.split(|v| *v == ENCODE_SEPARATOR).collect();

    values.get(position).map(|s| String::from_utf8_lossy(s).to_string())
}

#[derive(Clone)]
pub struct IndexeddbStore {
    name: String,
    pub(crate) inner: IdbDatabase,
    store_key: Option<StoreKey>,
}

impl std::fmt::Debug for IndexeddbStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexeddbStore").field("name", &self.name).finish()
    }
}

impl IndexeddbStore {
    async fn open_helper(name: String, store_key: Option<StoreKey>) -> Result<Self> {
        // Open my_db v1
        let mut db_req: OpenDbRequest = IdbDatabase::open_u32(&name, 1)?;
        db_req.set_on_upgrade_needed(Some(|evt: IdbVersionChangeEvent| -> Result<(), JsValue> {

            if evt.old_version < 1 {
                // migrating to version 1
                let db = evt.db();

                db.create_object_store("session")?;
                db.create_object_store("account_data")?;

                db.create_object_store("members")?;
                db.create_object_store("profiles")?;
                db.create_object_store("display_names")?;
                db.create_object_store("joined_user_ids")?;
                db.create_object_store("invited_user_ids")?;

                db.create_object_store("room_state")?;
                db.create_object_store("room_infos")?;
                db.create_object_store("presence")?;
                db.create_object_store("room_account_data")?;

                db.create_object_store("stripped_room_info")?;
                db.create_object_store("stripped_members")?;
                db.create_object_store("stripped_room_state")?;

                db.create_object_store("room_user_receipts")?;
                db.create_object_store("room_event_receipts")?;

                db.create_object_store("media")?;

                db.create_object_store("custom")?;

            }
            Ok(())
        }));

        let db: IdbDatabase = db_req.into_future().await?;

        Ok(Self {
            name,
            inner: db,
            store_key,
        })
    }

    pub async fn open() -> Result<Self> {
        IndexeddbStore::open_helper("state".to_owned, None)
    }

    pub async fn open_with_passphrase(name: String, passphrase: &str) -> Result<Self> {
        let path = format!("{:0}::matrix-sdk-state", name);

        let mut db_req: OpenDbRequest = IdbDatabase::open_u32(path, 1)?;
        db_req.set_on_upgrade_needed(Some(|evt: IdbVersionChangeEvent| -> Result<(), JsValue> {

            if evt.old_version < 1 {
                // migrating to version 1
                let db = evt.db();

                db.create_object_store("matrix-sdk-state")?;
            }
            Ok(())
        }));

        let db: IdbDatabase = db_req.into_future().await?;

        let tx: IdbTransaction = db.transaction_on_one_with_mode(path, IdbTransactionMode::Readwrite)?;
        let ob = tx.object_store("matrix-sdk-state").await?;


        let store_key: Option<DatabaseType> = ob
            .get("store_key")?
            .map(|k| serde_json::from_slice(&k).map_err(StoreError::Json))
            .transpose()?;

        let store_key = if let Some(key) = store_key {
            if let DatabaseType::Encrypted(k) = key {
                StoreKey::import(passphrase, k).map_err(|_| StoreError::StoreLocked)?
            } else {
                return Err(StoreError::UnencryptedStore);
            }
        } else {
            let key = StoreKey::new().map_err::<StoreError, _>(|e| e.into())?;
            let encrypted_key = DatabaseType::Encrypted(
                key.export(passphrase).map_err::<StoreError, _>(|e| e.into())?,
            );
            ob.put_key_val("store_key", serde_json::to_vec(&encrypted_key)?)?;
            key
        };

        IndexeddbStore::open_helper(name, Some(store_key))
    }

    pub async fn open_with_name(name: String) -> Result<Self> {
        IndexeddbStore::open_helper(name, None)
    }

    fn serialize_event(&self, event: &impl Serialize) -> Result<Vec<u8>, SerializationError> {
        if let Some(key) = self.store_key {
            let encrypted = key.encrypt(event)?;
            Ok(serde_json::to_vec(&encrypted)?)
        } else {
            Ok(serde_json::to_vec(event)?)
        }
    }

    fn deserialize_event<T: for<'b> Deserialize<'b>>(
        &self,
        event: &[u8],
    ) -> Result<T, SerializationError> {
        if let Some(key) = self.store_key {
            let encrypted: EncryptedEvent = serde_json::from_slice(event)?;
            Ok(key.decrypt(encrypted)?)
        } else {
            Ok(serde_json::from_slice(event)?)
        }
    }

    pub async fn save_filter(&self, filter_name: &str, filter_id: &str) -> Result<()> {
        self.session.insert(("filter", filter_name).encode(), filter_id)?;

        Ok(())
    }

    pub async fn get_filter(&self, filter_name: &str) -> Result<Option<String>> {
        Ok(self
            .session
            .get(("filter", filter_name).encode())?
            .map(|f| String::from_utf8_lossy(&f).to_string()))
    }

    pub async fn get_sync_token(&self) -> Result<Option<String>> {
        Ok(self
            .session
            .get("sync_token".encode())?
            .map(|t| String::from_utf8_lossy(&t).to_string()))
    }

    pub async fn save_changes(&self, changes: &StateChanges) -> Result<()> {
        unimplemented!()
        // let now = Instant::now();

        // let ret: Result<(), TransactionError<SerializationError>> = (
        //     &self.session,
        //     &self.account_data,
        //     &self.members,
        //     &self.profiles,
        //     &self.display_names,
        //     &self.joined_user_ids,
        //     &self.invited_user_ids,
        //     &self.room_info,
        //     &self.room_state,
        //     &self.room_account_data,
        //     &self.presence,
        //     &self.stripped_room_info,
        //     &self.stripped_members,
        //     &self.stripped_room_state,
        // )
        //     .transaction(
        //         |(
        //             session,
        //             account_data,
        //             members,
        //             profiles,
        //             display_names,
        //             joined,
        //             invited,
        //             rooms,
        //             state,
        //             room_account_data,
        //             presence,
        //             striped_rooms,
        //             stripped_members,
        //             stripped_state,
        //         )| {
        //             if let Some(s) = &changes.sync_token {
        //                 session.insert("sync_token".encode(), s.as_str())?;
        //             }

        //             for (room, events) in &changes.members {
        //                 let profile_changes = changes.profiles.get(room);

        //                 for event in events.values() {
        //                     let key = (room.as_str(), event.state_key.as_str()).encode();

        //                     match event.content.membership {
        //                         MembershipState::Join => {
        //                             joined.insert(key.as_slice(), event.state_key.as_str())?;
        //                             invited.remove(key.as_slice())?;
        //                         }
        //                         MembershipState::Invite => {
        //                             invited.insert(key.as_slice(), event.state_key.as_str())?;
        //                             joined.remove(key.as_slice())?;
        //                         }
        //                         _ => {
        //                             joined.remove(key.as_slice())?;
        //                             invited.remove(key.as_slice())?;
        //                         }
        //                     }

        //                     members.insert(
        //                         key.as_slice(),
        //                         self.serialize_event(&event)
        //                             .map_err(ConflictableTransactionError::Abort)?,
        //                     )?;

        //                     if let Some(profile) =
        //                         profile_changes.and_then(|p| p.get(&event.state_key))
        //                     {
        //                         profiles.insert(
        //                             key.as_slice(),
        //                             self.serialize_event(&profile)
        //                                 .map_err(ConflictableTransactionError::Abort)?,
        //                         )?;
        //                     }
        //                 }
        //             }

        //             for (room_id, ambiguity_maps) in &changes.ambiguity_maps {
        //                 for (display_name, map) in ambiguity_maps {
        //                     display_names.insert(
        //                         (room_id.as_str(), display_name.as_str()).encode(),
        //                         self.serialize_event(&map)
        //                             .map_err(ConflictableTransactionError::Abort)?,
        //                     )?;
        //                 }
        //             }

        //             for (event_type, event) in &changes.account_data {
        //                 account_data.insert(
        //                     event_type.as_str().encode(),
        //                     self.serialize_event(&event)
        //                         .map_err(ConflictableTransactionError::Abort)?,
        //                 )?;
        //             }

        //             for (room, events) in &changes.room_account_data {
        //                 for (event_type, event) in events {
        //                     room_account_data.insert(
        //                         (room.as_str(), event_type.as_str()).encode(),
        //                         self.serialize_event(&event)
        //                             .map_err(ConflictableTransactionError::Abort)?,
        //                     )?;
        //                 }
        //             }

        //             for (room, event_types) in &changes.state {
        //                 for (event_type, events) in event_types {
        //                     for (state_key, event) in events {
        //                         state.insert(
        //                             (room.as_str(), event_type.as_str(), state_key.as_str())
        //                                 .encode(),
        //                             self.serialize_event(&event)
        //                                 .map_err(ConflictableTransactionError::Abort)?,
        //                         )?;
        //                     }
        //                 }
        //             }

        //             for (room_id, room_info) in &changes.room_infos {
        //                 rooms.insert(
        //                     room_id.encode(),
        //                     self.serialize_event(room_info)
        //                         .map_err(ConflictableTransactionError::Abort)?,
        //                 )?;
        //             }

        //             for (sender, event) in &changes.presence {
        //                 presence.insert(
        //                     sender.encode(),
        //                     self.serialize_event(&event)
        //                         .map_err(ConflictableTransactionError::Abort)?,
        //                 )?;
        //             }

        //             for (room_id, info) in &changes.invited_room_info {
        //                 striped_rooms.insert(
        //                     room_id.encode(),
        //                     self.serialize_event(&info)
        //                         .map_err(ConflictableTransactionError::Abort)?,
        //                 )?;
        //             }

        //             for (room, events) in &changes.stripped_members {
        //                 for event in events.values() {
        //                     stripped_members.insert(
        //                         (room.as_str(), event.state_key.as_str()).encode(),
        //                         self.serialize_event(&event)
        //                             .map_err(ConflictableTransactionError::Abort)?,
        //                     )?;
        //                 }
        //             }

        //             for (room, event_types) in &changes.stripped_state {
        //                 for (event_type, events) in event_types {
        //                     for (state_key, event) in events {
        //                         stripped_state.insert(
        //                             (room.as_str(), event_type.as_str(), state_key.as_str())
        //                                 .encode(),
        //                             self.serialize_event(&event)
        //                                 .map_err(ConflictableTransactionError::Abort)?,
        //                         )?;
        //                     }
        //                 }
        //             }

        //             Ok(())
        //         },
        //     );

        // ret?;

        // let ret: Result<(), TransactionError<SerializationError>> =
        //     (&self.room_user_receipts, &self.room_event_receipts).transaction(
        //         |(room_user_receipts, room_event_receipts)| {
        //             for (room, content) in &changes.receipts {
        //                 for (event_id, receipts) in &content.0 {
        //                     for (receipt_type, receipts) in receipts {
        //                         for (user_id, receipt) in receipts {
        //                             // Add the receipt to the room user receipts
        //                             if let Some(old) = room_user_receipts.insert(
        //                                 (room.as_str(), receipt_type.as_ref(), user_id.as_str())
        //                                     .encode(),
        //                                 self.serialize_event(&(event_id, receipt))
        //                                     .map_err(ConflictableTransactionError::Abort)?,
        //                             )? {
        //                                 // Remove the old receipt from the room event receipts
        //                                 let (old_event, _): (EventId, Receipt) = self
        //                                     .deserialize_event(&old)
        //                                     .map_err(ConflictableTransactionError::Abort)?;
        //                                 room_event_receipts.remove(
        //                                     (
        //                                         room.as_str(),
        //                                         receipt_type.as_ref(),
        //                                         old_event.as_str(),
        //                                         user_id.as_str(),
        //                                     )
        //                                         .encode(),
        //                                 )?;
        //                             }

        //                             // Add the receipt to the room event receipts
        //                             room_event_receipts.insert(
        //                                 (
        //                                     room.as_str(),
        //                                     receipt_type.as_ref(),
        //                                     event_id.as_str(),
        //                                     user_id.as_str(),
        //                                 )
        //                                     .encode(),
        //                                 self.serialize_event(receipt)
        //                                     .map_err(ConflictableTransactionError::Abort)?,
        //                             )?;
        //                         }
        //                     }
        //                 }
        //             }

        //             Ok(())
        //         },
        //     );

        // ret?;

        // self.inner.flush_async().await?;

        // info!("Saved changes in {:?}", now.elapsed());

        // Ok(())
    }

    pub async fn get_presence_event(&self, user_id: &UserId) -> Result<Option<Raw<PresenceEvent>>> {
        let db = self.clone();
        let key = user_id.encode();
        spawn_blocking(move || {
            Ok(db.presence.get(key)?.map(|e| db.deserialize_event(&e)).transpose()?)
        })
        .await?
    }

    pub async fn get_state_event(
        &self,
        room_id: &RoomId,
        event_type: EventType,
        state_key: &str,
    ) -> Result<Option<Raw<AnySyncStateEvent>>> {
        let db = self.clone();
        let key = (room_id.as_str(), event_type.as_str(), state_key).encode();
        spawn_blocking(move || {
            Ok(db.room_state.get(key)?.map(|e| db.deserialize_event(&e)).transpose()?)
        })
        .await?
    }

    pub async fn get_state_events(
        &self,
        room_id: &RoomId,
        event_type: EventType,
    ) -> Result<Vec<Raw<AnySyncStateEvent>>> {
        let db = self.clone();
        let key = (room_id.as_str(), event_type.as_str()).encode();
        spawn_blocking(move || {
            Ok(db
                .room_state
                .scan_prefix(key)
                .flat_map(|e| e.map(|(_, e)| db.deserialize_event(&e)))
                .collect::<Result<_, _>>()?)
        })
        .await?
    }

    pub async fn get_profile(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
    ) -> Result<Option<RoomMemberEventContent>> {
        let db = self.clone();
        let key = (room_id.as_str(), user_id.as_str()).encode();
        spawn_blocking(move || {
            Ok(db.profiles.get(key)?.map(|p| db.deserialize_event(&p)).transpose()?)
        })
        .await?
    }

    pub async fn get_member_event(
        &self,
        room_id: &RoomId,
        state_key: &UserId,
    ) -> Result<Option<MemberEvent>> {
        let db = self.clone();
        let key = (room_id.as_str(), state_key.as_str()).encode();
        spawn_blocking(move || {
            Ok(db.members.get(key)?.map(|v| db.deserialize_event(&v)).transpose()?)
        })
        .await?
    }

    pub async fn get_user_ids_stream(
        &self,
        room_id: &RoomId,
    ) -> Result<impl Stream<Item = Result<UserId>>> {
        let decode = |key: &[u8]| -> Result<UserId> {
            let mut iter = key.split(|c| c == &ENCODE_SEPARATOR);
            // Our key is a the room id separated from the user id by a null
            // byte, discard the first value of the split.
            iter.next();

            let user_id = iter.next().expect("User ids weren't properly encoded");

            Ok(UserId::try_from(String::from_utf8_lossy(user_id).to_string())?)
        };

        let members = self.members.clone();
        let key = room_id.encode();

        spawn_blocking(move || stream::iter(members.scan_prefix(key).map(move |u| decode(&u?.0))))
            .await
            .map_err(Into::into)
    }

    pub async fn get_invited_user_ids(
        &self,
        room_id: &RoomId,
    ) -> Result<impl Stream<Item = Result<UserId>>> {
        let db = self.clone();
        let key = room_id.encode();
        spawn_blocking(move || {
            stream::iter(db.invited_user_ids.scan_prefix(key).map(|u| {
                UserId::try_from(String::from_utf8_lossy(&u?.1).to_string())
                    .map_err(StoreError::Identifier)
            }))
        })
        .await
        .map_err(Into::into)
    }

    pub async fn get_joined_user_ids(
        &self,
        room_id: &RoomId,
    ) -> Result<impl Stream<Item = Result<UserId>>> {
        let db = self.clone();
        let key = room_id.encode();
        spawn_blocking(move || {
            stream::iter(db.joined_user_ids.scan_prefix(key).map(|u| {
                UserId::try_from(String::from_utf8_lossy(&u?.1).to_string())
                    .map_err(StoreError::Identifier)
            }))
        })
        .await
        .map_err(Into::into)
    }

    pub async fn get_room_infos(&self) -> Result<impl Stream<Item = Result<RoomInfo>>> {
        let db = self.clone();
        spawn_blocking(move || {
            stream::iter(
                db.room_info.iter().map(move |r| db.deserialize_event(&r?.1).map_err(|e| e.into())),
            )
        })
        .await
        .map_err(Into::into)
    }

    pub async fn get_stripped_room_infos(&self) -> Result<impl Stream<Item = Result<RoomInfo>>> {
        let db = self.clone();
        spawn_blocking(move || {
            stream::iter(
                db.stripped_room_info
                    .iter()
                    .map(move |r| db.deserialize_event(&r?.1).map_err(|e| e.into())),
            )
        })
        .await
        .map_err(Into::into)
    }

    pub async fn get_users_with_display_name(
        &self,
        room_id: &RoomId,
        display_name: &str,
    ) -> Result<BTreeSet<UserId>> {
        let db = self.clone();
        let key = (room_id.as_str(), display_name).encode();
        spawn_blocking(move || {
            Ok(db
                .display_names
                .get(key)?
                .map(|m| db.deserialize_event(&m))
                .transpose()?
                .unwrap_or_default())
        })
        .await?
    }

    pub async fn get_account_data_event(
        &self,
        event_type: EventType,
    ) -> Result<Option<Raw<AnyGlobalAccountDataEvent>>> {
        let db = self.clone();
        let key = event_type.encode();
        spawn_blocking(move || {
            Ok(db.account_data.get(key)?.map(|m| db.deserialize_event(&m)).transpose()?)
        })
        .await?
    }

    pub async fn get_room_account_data_event(
        &self,
        room_id: &RoomId,
        event_type: EventType,
    ) -> Result<Option<Raw<AnyRoomAccountDataEvent>>> {
        let db = self.clone();
        let key = (room_id.as_str(), event_type.as_str()).encode();
        spawn_blocking(move || {
            Ok(db.room_account_data.get(key)?.map(|m| db.deserialize_event(&m)).transpose()?)
        })
        .await?
    }

    async fn get_user_room_receipt_event(
        &self,
        room_id: &RoomId,
        receipt_type: ReceiptType,
        user_id: &UserId,
    ) -> Result<Option<(EventId, Receipt)>> {
        let db = self.clone();
        let key = (room_id.as_str(), receipt_type.as_ref(), user_id.as_str()).encode();
        spawn_blocking(move || {
            Ok(db.room_user_receipts.get(key)?.map(|m| db.deserialize_event(&m)).transpose()?)
        })
        .await?
    }

    async fn get_event_room_receipt_events(
        &self,
        room_id: &RoomId,
        receipt_type: ReceiptType,
        event_id: &EventId,
    ) -> Result<Vec<(UserId, Receipt)>> {
        let db = self.clone();
        let key = (room_id.as_str(), receipt_type.as_ref(), event_id.as_str()).encode();
        spawn_blocking(move || {
            db.room_event_receipts
                .scan_prefix(key)
                .map(|u| {
                    u.map_err(StoreError::Indexeddb).and_then(|(key, value)| {
                        db.deserialize_event(&value)
                            // TODO remove this unwrapping
                            .map(|receipt| {
                                (decode_key_value(&key, 3).unwrap().try_into().unwrap(), receipt)
                            })
                            .map_err(Into::into)
                    })
                })
                .collect()
        })
        .await?
    }

    async fn add_media_content(&self, request: &MediaRequest, data: Vec<u8>) -> Result<()> {
        self.media.insert(
            (request.media_type.unique_key().as_str(), request.format.unique_key().as_str())
                .encode(),
            data,
        )?;

        self.inner.flush_async().await?;

        Ok(())
    }

    async fn get_media_content(&self, request: &MediaRequest) -> Result<Option<Vec<u8>>> {
        let db = self.clone();
        let key = (request.media_type.unique_key().as_str(), request.format.unique_key().as_str())
            .encode();

        spawn_blocking(move || Ok(db.media.get(key)?.map(|m| m.to_vec()))).await?
    }

    async fn get_custom_value(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let custom = self.custom.clone();
        let key = key.to_owned();
        spawn_blocking(move || Ok(custom.get(key)?.map(|v| v.to_vec()))).await?
    }

    async fn set_custom_value(&self, key: &[u8], value: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let ret = self.custom.insert(key, value)?.map(|v| v.to_vec());
        self.inner.flush_async().await?;

        Ok(ret)
    }

    async fn remove_media_content(&self, request: &MediaRequest) -> Result<()> {
        self.media.remove(
            (request.media_type.unique_key().as_str(), request.format.unique_key().as_str())
                .encode(),
        )?;

        Ok(())
    }

    async fn remove_media_content_for_uri(&self, uri: &MxcUri) -> Result<()> {
        unimplemented!()
    }
}

#[async_trait(?Send)]
impl StateStore for IndexeddbStore {
    async fn save_filter(&self, filter_name: &str, filter_id: &str) -> Result<()> {
        self.save_filter(filter_name, filter_id).await
    }

    async fn save_changes(&self, changes: &StateChanges) -> Result<()> {
        self.save_changes(changes).await
    }

    async fn get_filter(&self, filter_id: &str) -> Result<Option<String>> {
        self.get_filter(filter_id).await
    }

    async fn get_sync_token(&self) -> Result<Option<String>> {
        self.get_sync_token().await
    }

    async fn get_presence_event(&self, user_id: &UserId) -> Result<Option<Raw<PresenceEvent>>> {
        self.get_presence_event(user_id).await
    }

    async fn get_state_event(
        &self,
        room_id: &RoomId,
        event_type: EventType,
        state_key: &str,
    ) -> Result<Option<Raw<AnySyncStateEvent>>> {
        self.get_state_event(room_id, event_type, state_key).await
    }

    async fn get_state_events(
        &self,
        room_id: &RoomId,
        event_type: EventType,
    ) -> Result<Vec<Raw<AnySyncStateEvent>>> {
        self.get_state_events(room_id, event_type).await
    }

    async fn get_profile(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
    ) -> Result<Option<RoomMemberEventContent>> {
        self.get_profile(room_id, user_id).await
    }

    async fn get_member_event(
        &self,
        room_id: &RoomId,
        state_key: &UserId,
    ) -> Result<Option<MemberEvent>> {
        self.get_member_event(room_id, state_key).await
    }

    async fn get_user_ids(&self, room_id: &RoomId) -> Result<Vec<UserId>> {
        self.get_user_ids_stream(room_id).await?.try_collect().await
    }

    async fn get_invited_user_ids(&self, room_id: &RoomId) -> Result<Vec<UserId>> {
        self.get_invited_user_ids(room_id).await?.try_collect().await
    }

    async fn get_joined_user_ids(&self, room_id: &RoomId) -> Result<Vec<UserId>> {
        self.get_joined_user_ids(room_id).await?.try_collect().await
    }

    async fn get_room_infos(&self) -> Result<Vec<RoomInfo>> {
        self.get_room_infos().await?.try_collect().await
    }

    async fn get_stripped_room_infos(&self) -> Result<Vec<RoomInfo>> {
        self.get_stripped_room_infos().await?.try_collect().await
    }

    async fn get_users_with_display_name(
        &self,
        room_id: &RoomId,
        display_name: &str,
    ) -> Result<BTreeSet<UserId>> {
        self.get_users_with_display_name(room_id, display_name).await
    }

    async fn get_account_data_event(
        &self,
        event_type: EventType,
    ) -> Result<Option<Raw<AnyGlobalAccountDataEvent>>> {
        self.get_account_data_event(event_type).await
    }

    async fn get_room_account_data_event(
        &self,
        room_id: &RoomId,
        event_type: EventType,
    ) -> Result<Option<Raw<AnyRoomAccountDataEvent>>> {
        self.get_room_account_data_event(room_id, event_type).await
    }

    async fn get_user_room_receipt_event(
        &self,
        room_id: &RoomId,
        receipt_type: ReceiptType,
        user_id: &UserId,
    ) -> Result<Option<(EventId, Receipt)>> {
        self.get_user_room_receipt_event(room_id, receipt_type, user_id).await
    }

    async fn get_event_room_receipt_events(
        &self,
        room_id: &RoomId,
        receipt_type: ReceiptType,
        event_id: &EventId,
    ) -> Result<Vec<(UserId, Receipt)>> {
        self.get_event_room_receipt_events(room_id, receipt_type, event_id).await
    }

    async fn get_custom_value(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.get_custom_value(key).await
    }

    async fn set_custom_value(&self, key: &[u8], value: Vec<u8>) -> Result<Option<Vec<u8>>> {
        self.set_custom_value(key, value).await
    }

    async fn add_media_content(&self, request: &MediaRequest, data: Vec<u8>) -> Result<()> {
        self.add_media_content(request, data).await
    }

    async fn get_media_content(&self, request: &MediaRequest) -> Result<Option<Vec<u8>>> {
        self.get_media_content(request).await
    }

    async fn remove_media_content(&self, request: &MediaRequest) -> Result<()> {
        self.remove_media_content(request).await
    }

    async fn remove_media_content_for_uri(&self, uri: &MxcUri) -> Result<()> {
        self.remove_media_content_for_uri(uri).await
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use matrix_sdk_test::async_test;
    use ruma::{
        api::client::r0::media::get_content_thumbnail::Method,
        event_id,
        events::{
            room::{
                member::{MembershipState, RoomMemberEventContent},
                power_levels::RoomPowerLevelsEventContent,
            },
            AnySyncStateEvent, EventType, Unsigned,
        },
        mxc_uri,
        receipt::ReceiptType,
        room_id,
        serde::Raw,
        uint, user_id, EventId, MilliSecondsSinceUnixEpoch, UserId,
    };
    use serde_json::json;

    use super::{Result, IndexeddbStore, StateChanges};
    use crate::{
        deserialized_responses::MemberEvent,
        media::{MediaFormat, MediaRequest, MediaThumbnailSize, MediaType},
        StateStore,
    };

    fn user_id() -> UserId {
        user_id!("@example:localhost")
    }

    fn power_level_event() -> Raw<AnySyncStateEvent> {
        let content = RoomPowerLevelsEventContent::default();

        let event = json!({
            "event_id": EventId::try_from("$h29iv0s8:example.com").unwrap(),
            "content": content,
            "sender": user_id(),
            "type": "m.room.power_levels",
            "origin_server_ts": 0u64,
            "state_key": "",
            "unsigned": Unsigned::default(),
        });

        serde_json::from_value(event).unwrap()
    }

    fn membership_event() -> MemberEvent {
        MemberEvent {
            event_id: EventId::try_from("$h29iv0s8:example.com").unwrap(),
            content: RoomMemberEventContent::new(MembershipState::Join),
            sender: user_id(),
            origin_server_ts: MilliSecondsSinceUnixEpoch::now(),
            state_key: user_id(),
            prev_content: None,
            unsigned: Unsigned::default(),
        }
    }

    #[async_test]
    async fn test_member_saving() {
        let store = IndexeddbStore::open().unwrap();
        let room_id = room_id!("!test:localhost");
        let user_id = user_id();

        assert!(store.get_member_event(&room_id, &user_id).await.unwrap().is_none());
        let mut changes = StateChanges::default();
        changes
            .members
            .entry(room_id.clone())
            .or_default()
            .insert(user_id.clone(), membership_event());

        store.save_changes(&changes).await.unwrap();
        assert!(store.get_member_event(&room_id, &user_id).await.unwrap().is_some());

        let members = store.get_user_ids(&room_id).await.unwrap();
        assert!(!members.is_empty())
    }

    #[async_test]
    async fn test_power_level_saving() {
        let store = IndexeddbStore::open().unwrap();
        let room_id = room_id!("!test:localhost");

        let raw_event = power_level_event();
        let event = raw_event.deserialize().unwrap();

        assert!(store
            .get_state_event(&room_id, EventType::RoomPowerLevels, "")
            .await
            .unwrap()
            .is_none());
        let mut changes = StateChanges::default();
        changes.add_state_event(&room_id, event, raw_event);

        store.save_changes(&changes).await.unwrap();
        assert!(store
            .get_state_event(&room_id, EventType::RoomPowerLevels, "")
            .await
            .unwrap()
            .is_some());
    }

    #[async_test]
    async fn test_receipts_saving() {
        let store = IndexeddbStore::open().unwrap();

        let room_id = room_id!("!test:localhost");

        let first_event_id = event_id!("$1435641916114394fHBLK:matrix.org");
        let second_event_id = event_id!("$fHBLK1435641916114394:matrix.org");

        let first_receipt_event = serde_json::from_value(json!({
            first_event_id.clone(): {
                "m.read": {
                    user_id(): {
                        "ts": 1436451550453u64
                    }
                }
            }
        }))
        .unwrap();

        let second_receipt_event = serde_json::from_value(json!({
            second_event_id.clone(): {
                "m.read": {
                    user_id(): {
                        "ts": 1436451551453u64
                    }
                }
            }
        }))
        .unwrap();

        assert!(store
            .get_user_room_receipt_event(&room_id, ReceiptType::Read, &user_id())
            .await
            .unwrap()
            .is_none());
        assert!(store
            .get_event_room_receipt_events(&room_id, ReceiptType::Read, &first_event_id)
            .await
            .unwrap()
            .is_empty());
        assert!(store
            .get_event_room_receipt_events(&room_id, ReceiptType::Read, &second_event_id)
            .await
            .unwrap()
            .is_empty());

        let mut changes = StateChanges::default();
        changes.add_receipts(&room_id, first_receipt_event);

        store.save_changes(&changes).await.unwrap();
        assert!(store
            .get_user_room_receipt_event(&room_id, ReceiptType::Read, &user_id())
            .await
            .unwrap()
            .is_some(),);
        assert_eq!(
            store
                .get_event_room_receipt_events(&room_id, ReceiptType::Read, &first_event_id)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .get_event_room_receipt_events(&room_id, ReceiptType::Read, &second_event_id)
            .await
            .unwrap()
            .is_empty());

        let mut changes = StateChanges::default();
        changes.add_receipts(&room_id, second_receipt_event);

        store.save_changes(&changes).await.unwrap();
        assert!(store
            .get_user_room_receipt_event(&room_id, ReceiptType::Read, &user_id())
            .await
            .unwrap()
            .is_some());
        assert!(store
            .get_event_room_receipt_events(&room_id, ReceiptType::Read, &first_event_id)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .get_event_room_receipt_events(&room_id, ReceiptType::Read, &second_event_id)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[async_test]
    async fn test_media_content() {
        let store = IndexeddbStore::open().unwrap();

        let uri = mxc_uri!("mxc://localhost/media");
        let content: Vec<u8> = "somebinarydata".into();

        let request_file =
            MediaRequest { media_type: MediaType::Uri(uri.clone()), format: MediaFormat::File };

        let request_thumbnail = MediaRequest {
            media_type: MediaType::Uri(uri.clone()),
            format: MediaFormat::Thumbnail(MediaThumbnailSize {
                method: Method::Crop,
                width: uint!(100),
                height: uint!(100),
            }),
        };

        assert!(store.get_media_content(&request_file).await.unwrap().is_none());
        assert!(store.get_media_content(&request_thumbnail).await.unwrap().is_none());

        store.add_media_content(&request_file, content.clone()).await.unwrap();
        assert!(store.get_media_content(&request_file).await.unwrap().is_some());

        store.remove_media_content(&request_file).await.unwrap();
        assert!(store.get_media_content(&request_file).await.unwrap().is_none());

        store.add_media_content(&request_file, content.clone()).await.unwrap();
        assert!(store.get_media_content(&request_file).await.unwrap().is_some());

        store.add_media_content(&request_thumbnail, content.clone()).await.unwrap();
        assert!(store.get_media_content(&request_thumbnail).await.unwrap().is_some());

        store.remove_media_content_for_uri(&uri).await.unwrap();
        assert!(store.get_media_content(&request_file).await.unwrap().is_none());
        assert!(store.get_media_content(&request_thumbnail).await.unwrap().is_none());
    }

    #[async_test]
    async fn test_custom_storage() -> Result<()> {
        let key = "my_key";
        let value = &[0, 1, 2, 3];
        let store = IndexeddbStore::open()?;

        store.set_custom_value(key.as_bytes(), value.to_vec()).await?;

        let read = store.get_custom_value(key.as_bytes()).await?;

        assert_eq!(Some(value.as_ref()), read.as_deref());

        Ok(())
    }
}
