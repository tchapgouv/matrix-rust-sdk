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

use dashmap::DashMap;
use matrix_sdk_common::{
    api::r0::to_device::DeviceIdOrAllDevices,
    events::room::{
        encrypted::{MegolmV1AesSha2Content, MegolmV1AesSha2ContentInit},
        history_visibility::HistoryVisibility,
        message::Relation,
    },
    uuid::Uuid,
};
use std::{
    cmp::max,
    collections::BTreeMap,
    fmt,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tracing::debug;

use matrix_sdk_common::{
    events::{
        room::{encrypted::EncryptedEventContent, encryption::EncryptionEventContent},
        AnyMessageEventContent, EventContent,
    },
    identifiers::{DeviceId, DeviceIdBox, EventEncryptionAlgorithm, RoomId, UserId},
    instant::Instant,
    locks::Mutex,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub use olm_rs::{
    account::IdentityKeys,
    session::{OlmMessage, PreKeyMessage},
    utility::OlmUtility,
};
use olm_rs::{
    errors::OlmGroupSessionError, outbound_group_session::OlmOutboundGroupSession, PicklingMode,
};

use crate::ToDeviceRequest;

use super::{
    super::{deserialize_instant, serialize_instant},
    GroupSessionKey,
};

const ROTATION_PERIOD: Duration = Duration::from_millis(604800000);
const ROTATION_MESSAGES: u64 = 100;

pub enum ShareState {
    NotShared,
    Shared(u32),
}

/// Settings for an encrypted room.
///
/// This determines the algorithm and rotation periods of a group session.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EncryptionSettings {
    /// The encryption algorithm that should be used in the room.
    pub algorithm: EventEncryptionAlgorithm,
    /// How long the session should be used before changing it.
    pub rotation_period: Duration,
    /// How many messages should be sent before changing the session.
    pub rotation_period_msgs: u64,
    /// The history visibilty of the room when the session was created.
    pub history_visibility: HistoryVisibility,
}

impl Default for EncryptionSettings {
    fn default() -> Self {
        Self {
            algorithm: EventEncryptionAlgorithm::MegolmV1AesSha2,
            rotation_period: ROTATION_PERIOD,
            rotation_period_msgs: ROTATION_MESSAGES,
            history_visibility: HistoryVisibility::Shared,
        }
    }
}

impl EncryptionSettings {
    /// Create new encryption settings using an `EncryptionEventContent` and a
    /// history visibility.
    pub fn new(content: EncryptionEventContent, history_visibility: HistoryVisibility) -> Self {
        let rotation_period: Duration = content
            .rotation_period_ms
            .map_or(ROTATION_PERIOD, |r| Duration::from_millis(r.into()));
        let rotation_period_msgs: u64 = content
            .rotation_period_msgs
            .map_or(ROTATION_MESSAGES, Into::into);

        Self {
            algorithm: content.algorithm,
            rotation_period,
            rotation_period_msgs,
            history_visibility,
        }
    }
}

/// Outbound group session.
///
/// Outbound group sessions are used to exchange room messages between a group
/// of participants. Outbound group sessions are used to encrypt the room
/// messages.
#[derive(Clone)]
pub struct OutboundGroupSession {
    inner: Arc<Mutex<OlmOutboundGroupSession>>,
    device_id: Arc<DeviceIdBox>,
    account_identity_keys: Arc<IdentityKeys>,
    session_id: Arc<str>,
    room_id: Arc<RoomId>,
    pub(crate) creation_time: Arc<Instant>,
    message_count: Arc<AtomicU64>,
    shared: Arc<AtomicBool>,
    invalidated: Arc<AtomicBool>,
    settings: Arc<EncryptionSettings>,
    pub(crate) shared_with_set: Arc<DashMap<UserId, DashMap<DeviceIdBox, u32>>>,
    to_share_with_set: Arc<DashMap<Uuid, (Arc<ToDeviceRequest>, u32)>>,
}

impl OutboundGroupSession {
    /// Create a new outbound group session for the given room.
    ///
    /// Outbound group sessions are used to encrypt room messages.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The id of the device that created this session.
    ///
    /// * `identity_keys` - The identity keys of the account that created this
    /// session.
    ///
    /// * `room_id` - The id of the room that the session is used in.
    ///
    /// * `settings` - Settings determining the algorithm and rotation period of
    /// the outbound group session.
    pub fn new(
        device_id: Arc<DeviceIdBox>,
        identity_keys: Arc<IdentityKeys>,
        room_id: &RoomId,
        settings: EncryptionSettings,
    ) -> Self {
        let session = OlmOutboundGroupSession::new();
        let session_id = session.session_id();

        OutboundGroupSession {
            inner: Arc::new(Mutex::new(session)),
            room_id: Arc::new(room_id.to_owned()),
            device_id,
            account_identity_keys: identity_keys,
            session_id: session_id.into(),
            creation_time: Arc::new(Instant::now()),
            message_count: Arc::new(AtomicU64::new(0)),
            shared: Arc::new(AtomicBool::new(false)),
            invalidated: Arc::new(AtomicBool::new(false)),
            settings: Arc::new(settings),
            shared_with_set: Arc::new(DashMap::new()),
            to_share_with_set: Arc::new(DashMap::new()),
        }
    }

    pub(crate) fn add_request(
        &self,
        request_id: Uuid,
        request: Arc<ToDeviceRequest>,
        message_index: u32,
    ) {
        self.to_share_with_set
            .insert(request_id, (request, message_index));
    }

    /// This should be called if an the user wishes to rotate this session.
    pub fn invalidate_session(&self) {
        self.invalidated.store(true, Ordering::Relaxed)
    }

    /// Get the encryption settings of this outbound session.
    pub fn settings(&self) -> &EncryptionSettings {
        &self.settings
    }

    /// Mark the request with the given request id as sent.
    ///
    /// This removes the request from the queue and marks the set of
    /// users/devices that received the session.
    pub fn mark_request_as_sent(&self, request_id: &Uuid) {
        if let Some((_, r)) = self.to_share_with_set.remove(request_id) {
            let user_pairs = r.0.messages.iter().map(|(u, v)| {
                (
                    u.clone(),
                    v.iter().filter_map(|d| {
                        if let DeviceIdOrAllDevices::DeviceId(d) = d.0 {
                            Some((d.clone(), r.1))
                        } else {
                            None
                        }
                    }),
                )
            });

            user_pairs.for_each(|(u, d)| {
                self.shared_with_set
                    .entry(u)
                    .or_insert_with(DashMap::new)
                    .extend(d);
            });

            if self.to_share_with_set.is_empty() {
                debug!(
                    session_id = self.session_id(),
                    room_id = self.room_id.as_str(),
                    "All m.room_key to-device requests were sent out, marking \
                        session as shared.",
                );
                self.mark_as_shared();
            }
        }
    }

    /// Encrypt the given plaintext using this session.
    ///
    /// Returns the encrypted ciphertext.
    ///
    /// # Arguments
    ///
    /// * `plaintext` - The plaintext that should be encrypted.
    pub(crate) async fn encrypt_helper(&self, plaintext: String) -> String {
        let session = self.inner.lock().await;
        self.message_count.fetch_add(1, Ordering::SeqCst);
        session.encrypt(plaintext)
    }

    /// Encrypt a room message for the given room.
    ///
    /// Beware that a group session needs to be shared before this method can be
    /// called using the `share_group_session()` method.
    ///
    /// Since group sessions can expire or become invalid if the room membership
    /// changes client authors should check with the
    /// `should_share_group_session()` method if a new group session needs to
    /// be shared.
    ///
    /// # Arguments
    ///
    /// * `content` - The plaintext content of the message that should be
    /// encrypted.
    ///
    /// # Panics
    ///
    /// Panics if the content can't be serialized.
    pub async fn encrypt(&self, content: AnyMessageEventContent) -> EncryptedEventContent {
        let json_content = json!({
            "content": content,
            "room_id": &*self.room_id,
            "type": content.event_type(),
        });

        let relates_to: Option<Relation> = json_content
            .get("content")
            .map(|c| {
                c.get("m.relates_to")
                    .cloned()
                    .map(|r| serde_json::from_value(r).ok())
            })
            .flatten()
            .flatten();

        let plaintext = json_content.to_string();

        let ciphertext = self.encrypt_helper(plaintext).await;

        let mut encrypted_content: MegolmV1AesSha2Content = MegolmV1AesSha2ContentInit {
            ciphertext,
            sender_key: self.account_identity_keys.curve25519().to_owned(),
            session_id: self.session_id().to_owned(),
            device_id: (&*self.device_id).to_owned(),
        }
        .into();

        encrypted_content.relates_to = relates_to;

        EncryptedEventContent::MegolmV1AesSha2(encrypted_content)
    }

    /// Check if the session has expired and if it should be rotated.
    ///
    /// A session will expire after some time or if enough messages have been
    /// encrypted using it.
    pub fn expired(&self) -> bool {
        let count = self.message_count.load(Ordering::SeqCst);

        count >= self.settings.rotation_period_msgs
            || self.creation_time.elapsed()
                // Since the encryption settings are provided by users and not
                // checked someone could set a really low rotation period so
                // clamp it to an hour.
                >= max(self.settings.rotation_period, Duration::from_secs(3600))
    }

    /// Has the session been invalidated.
    pub fn invalidated(&self) -> bool {
        self.invalidated.load(Ordering::Relaxed)
    }

    /// Mark the session as shared.
    ///
    /// Messages shouldn't be encrypted with the session before it has been
    /// shared.
    pub fn mark_as_shared(&self) {
        self.shared.store(true, Ordering::Relaxed);
    }

    /// Check if the session has been marked as shared.
    pub fn shared(&self) -> bool {
        self.shared.load(Ordering::Relaxed)
    }

    /// Get the session key of this session.
    ///
    /// A session key can be used to to create an `InboundGroupSession`.
    pub async fn session_key(&self) -> GroupSessionKey {
        let session = self.inner.lock().await;
        GroupSessionKey(session.session_key())
    }

    /// Get the room id of the room this session belongs to.
    pub fn room_id(&self) -> &RoomId {
        &self.room_id
    }

    /// Returns the unique identifier for this session.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get the current message index for this session.
    ///
    /// Each message is sent with an increasing index. This returns the
    /// message index that will be used for the next encrypted message.
    pub async fn message_index(&self) -> u32 {
        let session = self.inner.lock().await;
        session.session_message_index()
    }

    /// Get the outbound group session key as a json value that can be sent as a
    /// m.room_key.
    pub async fn as_json(&self) -> Value {
        json!({
            "algorithm": EventEncryptionAlgorithm::MegolmV1AesSha2,
            "room_id": &*self.room_id,
            "session_id": &*self.session_id,
            "session_key": self.session_key().await,
            "chain_index": self.message_index().await,
        })
    }

    /// Has or will the session be shared with the given user/device pair.
    pub(crate) fn is_shared_with(&self, user_id: &UserId, device_id: &DeviceId) -> ShareState {
        // Check if we shared the session.
        let shared_state = self
            .shared_with_set
            .get(user_id)
            .and_then(|d| d.get(device_id).map(|m| ShareState::Shared(*m.value())));

        if let Some(state) = shared_state {
            state
        } else {
            // If we haven't shared the session, check if we're going to share
            // the session.
            let device_id = DeviceIdOrAllDevices::DeviceId(device_id.into());

            // Find the first request that contains the given user id and
            // device id.
            let shared = self.to_share_with_set.iter().find_map(|item| {
                let request = &item.value().0;
                let message_index = item.value().1;

                if request
                    .messages
                    .get(user_id)
                    .map(|e| e.contains_key(&device_id))
                    .unwrap_or(false)
                {
                    Some(ShareState::Shared(message_index))
                } else {
                    None
                }
            });

            shared.unwrap_or(ShareState::NotShared)
        }
    }

    /// Mark that the session was shared with the given user/device pair.
    #[cfg(test)]
    pub fn mark_shared_with(&self, user_id: &UserId, device_id: &DeviceId) {
        self.shared_with_set
            .entry(user_id.to_owned())
            .or_insert_with(DashMap::new)
            .insert(device_id.to_owned(), 0);
    }

    /// Get the list of requests that need to be sent out for this session to be
    /// marked as shared.
    pub(crate) fn pending_requests(&self) -> Vec<Arc<ToDeviceRequest>> {
        self.to_share_with_set
            .iter()
            .map(|i| i.value().0.clone())
            .collect()
    }

    /// Restore a Session from a previously pickled string.
    ///
    /// Returns the restored group session or a `OlmGroupSessionError` if there
    /// was an error.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The device id of the device that created this session.
    ///     Put differently, our own device id.
    ///
    /// * `identity_keys` - The identity keys of the device that created this
    ///     session, our own identity keys.
    ///
    /// * `pickle` - The pickled version of the `OutboundGroupSession`.
    ///
    /// * `pickle_mode` - The mode that was used to pickle the session, either
    /// an unencrypted mode or an encrypted using passphrase.
    pub fn from_pickle(
        device_id: Arc<DeviceIdBox>,
        identity_keys: Arc<IdentityKeys>,
        pickle: PickledOutboundGroupSession,
        pickling_mode: PicklingMode,
    ) -> Result<Self, OlmGroupSessionError> {
        let inner = OlmOutboundGroupSession::unpickle(pickle.pickle.0, pickling_mode)?;
        let session_id = inner.session_id();

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
            device_id,
            account_identity_keys: identity_keys,
            session_id: session_id.into(),
            room_id: pickle.room_id,
            creation_time: pickle.creation_time.into(),
            message_count: AtomicU64::from(pickle.message_count).into(),
            shared: AtomicBool::from(pickle.shared).into(),
            invalidated: AtomicBool::from(pickle.invalidated).into(),
            settings: pickle.settings,
            shared_with_set: Arc::new(
                pickle
                    .shared_with_set
                    .into_iter()
                    .map(|(k, v)| (k, v.into_iter().collect()))
                    .collect(),
            ),
            to_share_with_set: Arc::new(pickle.requests.into_iter().collect()),
        })
    }

    /// Store the group session as a base64 encoded string and associated data
    /// belonging to the session.
    ///
    /// # Arguments
    ///
    /// * `pickle_mode` - The mode that should be used to pickle the group session,
    /// either an unencrypted mode or an encrypted using passphrase.
    pub async fn pickle(&self, pickling_mode: PicklingMode) -> PickledOutboundGroupSession {
        let pickle: OutboundGroupSessionPickle =
            self.inner.lock().await.pickle(pickling_mode).into();

        PickledOutboundGroupSession {
            pickle,
            room_id: self.room_id.clone(),
            settings: self.settings.clone(),
            creation_time: *self.creation_time,
            message_count: self.message_count.load(Ordering::SeqCst),
            shared: self.shared(),
            invalidated: self.invalidated(),
            shared_with_set: self
                .shared_with_set
                .iter()
                .map(|u| {
                    (
                        u.key().clone(),
                        #[allow(clippy::map_clone)]
                        u.value()
                            .iter()
                            .map(|d| (d.key().clone(), *d.value()))
                            .collect(),
                    )
                })
                .collect(),
            requests: self
                .to_share_with_set
                .iter()
                .map(|r| (*r.key(), r.value().clone()))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutboundGroupSessionPickle(String);

impl From<String> for OutboundGroupSessionPickle {
    fn from(p: String) -> Self {
        Self(p)
    }
}

#[cfg(not(tarpaulin_include))]
impl std::fmt::Debug for OutboundGroupSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutboundGroupSession")
            .field("session_id", &self.session_id)
            .field("room_id", &self.room_id)
            .field("creation_time", &self.creation_time)
            .field("message_count", &self.message_count)
            .finish()
    }
}

/// A pickled version of an `InboundGroupSession`.
///
/// Holds all the information that needs to be stored in a database to restore
/// an InboundGroupSession.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PickledOutboundGroupSession {
    /// The pickle string holding the OutboundGroupSession.
    pub pickle: OutboundGroupSessionPickle,
    /// The settings this session adheres to.
    pub settings: Arc<EncryptionSettings>,
    /// The room id this session is used for.
    pub room_id: Arc<RoomId>,
    /// The timestamp when this session was created.
    #[serde(
        deserialize_with = "deserialize_instant",
        serialize_with = "serialize_instant"
    )]
    pub creation_time: Instant,
    /// The number of messages this session has already encrypted.
    pub message_count: u64,
    /// Is the session shared.
    pub shared: bool,
    /// Has the session been invalidated.
    pub invalidated: bool,
    /// The set of users the session has been already shared with.
    pub shared_with_set: BTreeMap<UserId, BTreeMap<DeviceIdBox, u32>>,
    /// Requests that need to be sent out to share the session.
    pub requests: BTreeMap<Uuid, (Arc<ToDeviceRequest>, u32)>,
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use matrix_sdk_common::{
        events::room::{encryption::EncryptionEventContent, history_visibility::HistoryVisibility},
        identifiers::EventEncryptionAlgorithm,
        uint,
    };

    use super::{EncryptionSettings, ROTATION_MESSAGES, ROTATION_PERIOD};

    #[test]
    fn encryption_settings_conversion() {
        let mut content = EncryptionEventContent::new(EventEncryptionAlgorithm::MegolmV1AesSha2);
        let settings = EncryptionSettings::new(content.clone(), HistoryVisibility::Joined);

        assert_eq!(settings.rotation_period, ROTATION_PERIOD);
        assert_eq!(settings.rotation_period_msgs, ROTATION_MESSAGES);

        content.rotation_period_ms = Some(uint!(3600));
        content.rotation_period_msgs = Some(uint!(500));

        let settings = EncryptionSettings::new(content, HistoryVisibility::Shared);

        assert_eq!(settings.rotation_period, Duration::from_millis(3600));
        assert_eq!(settings.rotation_period_msgs, 500);
    }
}
