// Copyright 2020 Damir Jelić
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

use std::collections::HashMap;
#[cfg(feature = "encryption")]
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

#[cfg(feature = "encryption")]
use std::result::Result as StdResult;

use crate::api::r0 as api;
use crate::error::Result;
use crate::events::collections::all::{RoomEvent, StateEvent};
use crate::events::presence::PresenceEvent;
// `NonRoomEvent` is what it is aliased as
use crate::events::collections::only::Event as NonRoomEvent;
use crate::events::ignored_user_list::IgnoredUserListEvent;
use crate::events::push_rules::{PushRulesEvent, Ruleset};
use crate::events::EventResult;
use crate::identifiers::RoomAliasId;
use crate::models::Room;
use crate::session::Session;
use crate::EventEmitter;

use tokio::sync::Mutex;

#[cfg(feature = "encryption")]
use crate::crypto::{OlmMachine, OneTimeKeys};
#[cfg(feature = "encryption")]
use ruma_client_api::r0::keys::{
    claim_keys::Response as KeysClaimResponse, get_keys::Response as KeysQueryResponse,
    upload_keys::Response as KeysUploadResponse, DeviceKeys, KeyAlgorithm,
};
use ruma_identifiers::RoomId;
#[cfg(feature = "encryption")]
use ruma_identifiers::{DeviceId, UserId as RumaUserId};

pub type Token = String;
pub type UserId = String;

#[derive(Debug, Default)]
/// `RoomName` allows the calculation of a text room name.
pub struct RoomName {
    /// The displayed name of the room.
    name: Option<String>,
    /// The canonical alias of the room ex. `#room-name:example.com` and port number.
    canonical_alias: Option<RoomAliasId>,
    /// List of `RoomAliasId`s the room has been given.
    aliases: Vec<RoomAliasId>,
}

/// A no IO Client implementation.
///
/// This Client is a state machine that receives responses and events and
/// accordingly updates it's state.
pub struct Client {
    /// The current client session containing our user id, device id and access
    /// token.
    pub session: Option<Session>,
    /// The current sync token that should be used for the next sync call.
    pub sync_token: Option<Token>,
    /// A map of the rooms our user is joined in.
    pub joined_rooms: HashMap<String, Arc<Mutex<Room>>>,
    /// A list of ignored users.
    pub ignored_users: Vec<UserId>,
    /// The push ruleset for the logged in user.
    pub push_ruleset: Option<Ruleset>,
    /// Any implementor of EventEmitter will act as the callbacks for various
    /// events.
    pub event_emitter: Option<Arc<Mutex<Box<dyn EventEmitter>>>>,

    #[cfg(feature = "encryption")]
    olm: Arc<Mutex<Option<OlmMachine>>>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("session", &self.session)
            .field("sync_token", &self.sync_token)
            .field("joined_rooms", &self.joined_rooms)
            .field("ignored_users", &self.ignored_users)
            .field("push_ruleset", &self.push_ruleset)
            .field("event_emitter", &"EventEmitter<...>")
            .finish()
    }
}

impl Client {
    /// Create a new client.
    ///
    /// # Arguments
    ///
    /// * `session` - An optional session if the user already has one from a
    /// previous login call.
    pub fn new(session: Option<Session>) -> Result<Self> {
        #[cfg(feature = "encryption")]
        let olm = match &session {
            Some(s) => Some(OlmMachine::new(&s.user_id, &s.device_id)?),
            None => None,
        };

        Ok(Client {
            session,
            sync_token: None,
            joined_rooms: HashMap::new(),
            ignored_users: Vec::new(),
            push_ruleset: None,
            event_emitter: None,
            #[cfg(feature = "encryption")]
            olm: Arc::new(Mutex::new(olm)),
        })
    }

    /// Is the client logged in.
    pub fn logged_in(&self) -> bool {
        self.session.is_some()
    }

    /// Add `EventEmitter` to `Client`.
    ///
    /// The methods of `EventEmitter` are called when the respective `RoomEvents` occur.
    pub async fn add_event_emitter(
        &mut self,
        emitter: Arc<tokio::sync::Mutex<Box<dyn EventEmitter>>>,
    ) {
        self.event_emitter = Some(emitter);
    }

    /// Receive a login response and update the session of the client.
    ///
    /// # Arguments
    ///
    /// * `response` - A successful login response that contains our access token
    /// and device id.
    pub async fn receive_login_response(
        &mut self,
        response: &api::session::login::Response,
    ) -> Result<()> {
        let session = Session {
            access_token: response.access_token.clone(),
            device_id: response.device_id.clone(),
            user_id: response.user_id.clone(),
        };
        self.session = Some(session);

        #[cfg(feature = "encryption")]
        {
            let mut olm = self.olm.lock().await;
            *olm = Some(OlmMachine::new(&response.user_id, &response.device_id)?);
        }

        Ok(())
    }

    pub(crate) async fn calculate_room_name(&self, room_id: &str) -> Option<String> {
        if let Some(room) = self.joined_rooms.get(room_id) {
            let room = room.lock().await;
            Some(room.room_name.calculate_name(room_id, &room.members))
        } else {
            None
        }
    }

    pub(crate) async fn calculate_room_names(&self) -> Vec<String> {
        let mut res = Vec::new();
        for (id, room) in &self.joined_rooms {
            let room = room.lock().await;
            res.push(room.room_name.calculate_name(id, &room.members))
        }
        res
    }

    pub(crate) fn get_or_create_room(&mut self, room_id: &str) -> &mut Arc<Mutex<Room>> {
        #[allow(clippy::or_fun_call)]
        self.joined_rooms
            .entry(room_id.to_string())
            .or_insert(Arc::new(Mutex::new(Room::new(
                room_id,
                &self
                    .session
                    .as_ref()
                    .expect("Receiving events while not being logged in")
                    .user_id
                    .to_string(),
            ))))
    }

    pub(crate) fn get_room(&self, room_id: &str) -> Option<&Arc<Mutex<Room>>> {
        self.joined_rooms.get(room_id)
    }

    /// Handle a m.ignored_user_list event, updating the room state if necessary.
    ///
    /// Returns true if the room name changed, false otherwise.
    pub(crate) fn handle_ignored_users(&mut self, event: &IgnoredUserListEvent) -> bool {
        // FIXME when UserId becomes more like a &str wrapper in ruma-identifiers
        if self.ignored_users
            == event
                .content
                .ignored_users
                .iter()
                .map(|u| u.to_string())
                .collect::<Vec<String>>()
        {
            false
        } else {
            self.ignored_users = event
                .content
                .ignored_users
                .iter()
                .map(|u| u.to_string())
                .collect();
            true
        }
    }

    /// Handle a m.ignored_user_list event, updating the room state if necessary.
    ///
    /// Returns true if the room name changed, false otherwise.
    pub(crate) fn handle_push_rules(&mut self, event: &PushRulesEvent) -> bool {
        // TODO this is basically a stub
        if self.push_ruleset.as_ref() == Some(&event.content.global) {
            false
        } else {
            self.push_ruleset = Some(event.content.global.clone());
            true
        }
    }

    /// Receive a timeline event for a joined room and update the client state.
    ///
    /// If the event was a encrypted room event and decryption was successful
    /// the decrypted event will be returned, otherwise None.
    ///
    /// # Arguments
    ///
    /// * `room_id` - The unique id of the room the event belongs to.
    ///
    /// * `event` - The event that should be handled by the client.
    pub async fn receive_joined_timeline_event(
        &mut self,
        room_id: &RoomId,
        event: &mut EventResult<RoomEvent>,
    ) -> Option<EventResult<RoomEvent>> {
        match event {
            EventResult::Ok(e) => {
                #[cfg(feature = "encryption")]
                let mut decrypted_event = None;
                #[cfg(not(feature = "encryption"))]
                let decrypted_event = None;

                #[cfg(feature = "encryption")]
                {
                    match e {
                        RoomEvent::RoomEncrypted(e) => {
                            e.room_id = Some(room_id.to_owned());
                            let mut olm = self.olm.lock().await;

                            if let Some(o) = &mut *olm {
                                decrypted_event = o.decrypt_room_event(e).await.ok();
                            }
                        }
                        _ => (),
                    }
                }

                let mut room = self.get_or_create_room(&room_id.to_string()).lock().await;
                room.receive_timeline_event(e);
                decrypted_event
            }
            _ => None,
        }
    }

    /// Receive a state event for a joined room and update the client state.
    ///
    /// Returns true if the membership list of the room changed, false
    /// otherwise.
    ///
    /// # Arguments
    ///
    /// * `room_id` - The unique id of the room the event belongs to.
    ///
    /// * `event` - The event that should be handled by the client.
    pub async fn receive_joined_state_event(&mut self, room_id: &str, event: &StateEvent) -> bool {
        let mut room = self.get_or_create_room(room_id).lock().await;
        room.receive_state_event(event)
    }

    /// Receive a presence event from a sync response and updates the client state.
    ///
    /// Returns true if the membership list of the room changed, false
    /// otherwise.
    ///
    /// # Arguments
    ///
    /// * `room_id` - The unique id of the room the event belongs to.
    ///
    /// * `event` - The event that should be handled by the client.
    pub async fn receive_presence_event(&mut self, room_id: &str, event: &PresenceEvent) -> bool {
        // this should be the room that was just created in the `Client::sync` loop.
        if let Some(room) = self.get_room(room_id) {
            let mut room = room.lock().await;
            room.receive_presence_event(event)
        } else {
            false
        }
    }

    /// Receive a presence event from a sync response and updates the client state.
    ///
    /// This will only update the user if found in the current room looped through by `AsyncClient::sync`.
    /// Returns true if the specific users presence has changed, false otherwise.
    ///
    /// # Arguments
    ///
    /// * `room_id` - The unique id of the room the event belongs to.
    ///
    /// * `event` - The presence event for a specified room member.
    pub async fn receive_account_data(&mut self, room_id: &str, event: &NonRoomEvent) -> bool {
        match event {
            NonRoomEvent::IgnoredUserList(iu) => self.handle_ignored_users(iu),
            NonRoomEvent::Presence(p) => self.receive_presence_event(room_id, p).await,
            NonRoomEvent::PushRules(pr) => self.handle_push_rules(pr),
            _ => false,
        }
    }

    /// Receive a response from a sync call.
    ///
    /// # Arguments
    ///
    /// * `response` - The response that we received after a successful sync.
    pub async fn receive_sync_response(
        &mut self,
        response: &mut api::sync::sync_events::IncomingResponse,
    ) {
        self.sync_token = Some(response.next_batch.clone());

        #[cfg(feature = "encryption")]
        {
            let mut olm = self.olm.lock().await;

            if let Some(o) = &mut *olm {
                o.receive_sync_response(response).await;

                // TODO once the base client deals with callbacks move this into the
                // part where we already iterate through the rooms to avoid yet
                // another room loop.
                for room in self.joined_rooms.values() {
                    let room = room.lock().await;
                    if !room.is_encrypted() {
                        continue;
                    }

                    o.update_tracked_users(room.members.keys()).await;
                }
            }
        }
    }

    /// Should account or one-time keys be uploaded to the server.
    #[cfg(feature = "encryption")]
    #[cfg_attr(docsrs, doc(cfg(feature = "encryption")))]
    pub async fn should_upload_keys(&self) -> bool {
        let olm = self.olm.lock().await;

        match &*olm {
            Some(o) => o.should_upload_keys().await,
            None => false,
        }
    }

    /// Should users be queried for their device keys.
    #[cfg(feature = "encryption")]
    #[cfg_attr(docsrs, doc(cfg(feature = "encryption")))]
    pub async fn should_query_keys(&self) -> bool {
        let olm = self.olm.lock().await;

        match &*olm {
            Some(o) => o.should_query_keys(),
            None => false,
        }
    }

    /// Get a tuple of device and one-time keys that need to be uploaded.
    ///
    /// Returns an empty error if no keys need to be uploaded.
    #[cfg(feature = "encryption")]
    #[cfg_attr(docsrs, doc(cfg(feature = "encryption")))]
    pub async fn get_missing_sessions(
        &self,
        users: impl Iterator<Item = &String>,
    ) -> HashMap<RumaUserId, HashMap<DeviceId, KeyAlgorithm>> {
        let mut olm = self.olm.lock().await;

        match &mut *olm {
            Some(o) => o.get_missing_sessions(users).await,
            None => HashMap::new(),
        }
    }

    /// Get a tuple of device and one-time keys that need to be uploaded.
    ///
    /// Returns an empty error if no keys need to be uploaded.
    #[cfg(feature = "encryption")]
    #[cfg_attr(docsrs, doc(cfg(feature = "encryption")))]
    pub async fn keys_for_upload(
        &self,
    ) -> StdResult<(Option<DeviceKeys>, Option<OneTimeKeys>), ()> {
        let olm = self.olm.lock().await;

        match &*olm {
            Some(o) => o.keys_for_upload().await,
            None => Err(()),
        }
    }

    /// Get the users that we need to query keys for.
    ///
    /// Returns an empty error if no keys need to be queried.
    #[cfg(feature = "encryption")]
    #[cfg_attr(docsrs, doc(cfg(feature = "encryption")))]
    pub async fn users_for_key_query(&self) -> StdResult<HashSet<String>, ()> {
        let olm = self.olm.lock().await;

        match &*olm {
            Some(o) => Ok(o.users_for_key_query()),
            None => Err(()),
        }
    }

    /// Receive a successful keys upload response.
    ///
    /// # Arguments
    ///
    /// * `response` - The keys upload response of the request that the client
    ///     performed.
    ///
    /// # Panics
    /// Panics if the client hasn't been logged in.
    #[cfg(feature = "encryption")]
    #[cfg_attr(docsrs, doc(cfg(feature = "encryption")))]
    pub async fn receive_keys_upload_response(&self, response: &KeysUploadResponse) -> Result<()> {
        let mut olm = self.olm.lock().await;

        let o = olm.as_mut().expect("Client isn't logged in.");
        o.receive_keys_upload_response(response).await?;
        Ok(())
    }

    /// Receive a successful keys claim response.
    ///
    /// # Arguments
    ///
    /// * `response` - The keys claim response of the request that the client
    /// performed.
    ///
    /// # Panics
    /// Panics if the client hasn't been logged in.
    #[cfg(feature = "encryption")]
    #[cfg_attr(docsrs, doc(cfg(feature = "encryption")))]
    pub async fn receive_keys_claim_response(&self, response: &KeysClaimResponse) -> Result<()> {
        let mut olm = self.olm.lock().await;

        let o = olm.as_mut().expect("Client isn't logged in.");
        o.receive_keys_claim_response(response).await?;
        Ok(())
    }

    /// Receive a successful keys query response.
    ///
    /// # Arguments
    ///
    /// * `response` - The keys query response of the request that the client
    /// performed.
    ///
    /// # Panics
    /// Panics if the client hasn't been logged in.
    #[cfg(feature = "encryption")]
    #[cfg_attr(docsrs, doc(cfg(feature = "encryption")))]
    pub async fn receive_keys_query_response(&self, response: &KeysQueryResponse) -> Result<()> {
        let mut olm = self.olm.lock().await;

        let o = olm.as_mut().expect("Client isn't logged in.");
        o.receive_keys_query_response(response).await?;
        // TODO notify our callers of new devices via some callback.
        Ok(())
    }

    pub(crate) async fn emit_timeline_event(&mut self, room_id: &RoomId, event: &mut RoomEvent) {
        match event {
            RoomEvent::RoomMember(mem) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_room_member(Arc::clone(&room), Arc::new(Mutex::new(mem.clone())))
                            .await;
                    }
                }
            }
            RoomEvent::RoomName(name) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_room_name(Arc::clone(&room), Arc::new(Mutex::new(name.clone())))
                            .await;
                    }
                }
            }
            RoomEvent::RoomCanonicalAlias(canonical) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_room_canonical_alias(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(canonical.clone())),
                            )
                            .await;
                    }
                }
            }
            RoomEvent::RoomAliases(aliases) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_room_aliases(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(aliases.clone())),
                            )
                            .await;
                    }
                }
            }
            RoomEvent::RoomAvatar(avatar) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_room_avatar(Arc::clone(&room), Arc::new(Mutex::new(avatar.clone())))
                            .await;
                    }
                }
            }
            RoomEvent::RoomMessage(msg) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_room_message(Arc::clone(&room), Arc::new(Mutex::new(msg.clone())))
                            .await;
                    }
                }
            }
            RoomEvent::RoomMessageFeedback(msg_feedback) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_room_message_feedback(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(msg_feedback.clone())),
                            )
                            .await;
                    }
                }
            }
            RoomEvent::RoomRedaction(redaction) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_room_redaction(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(redaction.clone())),
                            )
                            .await;
                    }
                }
            }
            RoomEvent::RoomPowerLevels(power) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_room_power_levels(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(power.clone())),
                            )
                            .await;
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) async fn emit_state_event(&mut self, room_id: &RoomId, event: &mut StateEvent) {
        match event {
            StateEvent::RoomMember(member) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_state_member(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(member.clone())),
                            )
                            .await;
                    }
                }
            }
            StateEvent::RoomName(name) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_state_name(Arc::clone(&room), Arc::new(Mutex::new(name.clone())))
                            .await;
                    }
                }
            }
            StateEvent::RoomCanonicalAlias(canonical) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_state_canonical_alias(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(canonical.clone())),
                            )
                            .await;
                    }
                }
            }
            StateEvent::RoomAliases(aliases) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_state_aliases(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(aliases.clone())),
                            )
                            .await;
                    }
                }
            }
            StateEvent::RoomAvatar(avatar) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_state_avatar(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(avatar.clone())),
                            )
                            .await;
                    }
                }
            }
            StateEvent::RoomPowerLevels(power) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_state_power_levels(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(power.clone())),
                            )
                            .await;
                    }
                }
            }
            StateEvent::RoomJoinRules(rules) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_state_join_rules(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(rules.clone())),
                            )
                            .await;
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) async fn emit_account_data_event(
        &mut self,
        room_id: &RoomId,
        event: &mut NonRoomEvent,
    ) {
        match event {
            NonRoomEvent::Presence(presence) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_account_presence(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(presence.clone())),
                            )
                            .await;
                    }
                }
            }
            NonRoomEvent::IgnoredUserList(ignored) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_account_ignored_users(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(ignored.clone())),
                            )
                            .await;
                    }
                }
            }
            NonRoomEvent::PushRules(rules) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_account_push_rules(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(rules.clone())),
                            )
                            .await;
                    }
                }
            }
            NonRoomEvent::FullyRead(full_read) => {
                if let Some(ee) = &self.event_emitter {
                    if let Some(room) = self.get_room(&room_id.to_string()) {
                        ee.lock()
                            .await
                            .on_account_data_fully_read(
                                Arc::clone(&room),
                                Arc::new(Mutex::new(full_read.clone())),
                            )
                            .await;
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) async fn emit_presence_event(
        &mut self,
        room_id: &RoomId,
        event: &mut PresenceEvent,
    ) {
        if let Some(ee) = &self.event_emitter {
            if let Some(room) = self.get_room(&room_id.to_string()) {
                ee.lock()
                    .await
                    .on_presence_event(Arc::clone(&room), Arc::new(Mutex::new(event.clone())))
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod test {

    use crate::identifiers::UserId;
    use crate::{AsyncClient, Session, SyncSettings};

    use mockito::{mock, Matcher};
    use url::Url;

    use std::convert::TryFrom;
    use std::str::FromStr;
    use std::time::Duration;

    #[tokio::test]
    async fn account_data() {
        let homeserver = Url::from_str(&mockito::server_url()).unwrap();

        let session = Session {
            access_token: "1234".to_owned(),
            user_id: UserId::try_from("@example:example.com").unwrap(),
            device_id: "DEVICEID".to_owned(),
        };

        let _m = mock(
            "GET",
            Matcher::Regex(r"^/_matrix/client/r0/sync\?.*$".to_string()),
        )
        .with_status(200)
        .with_body_from_file("tests/data/sync.json")
        .create();

        let mut client = AsyncClient::new(homeserver, Some(session)).unwrap();

        let sync_settings = SyncSettings::new().timeout(Duration::from_millis(3000));

        let _response = client.sync(sync_settings).await.unwrap();

        let bc = &client.base_client.read().await;
        assert_eq!(1, bc.ignored_users.len())
    }
}
