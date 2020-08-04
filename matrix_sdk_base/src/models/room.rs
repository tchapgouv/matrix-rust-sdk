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

#[cfg(feature = "messages")]
use std::ops::DerefMut;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    convert::TryFrom,
};

#[cfg(feature = "messages")]
use matrix_sdk_common::events::{
    room::redaction::SyncRedactionEvent, AnyPossiblyRedactedSyncMessageEvent, AnySyncMessageEvent,
};
use matrix_sdk_common::{
    api::r0::sync::sync_events::{RoomSummary, UnreadNotificationsCount},
    events::{
        presence::{PresenceEvent, PresenceEventContent},
        room::{
            aliases::AliasesEventContent,
            canonical_alias::CanonicalAliasEventContent,
            encryption::EncryptionEventContent,
            member::{MemberEventContent, MembershipChange, MembershipState},
            name::NameEventContent,
            power_levels::{NotificationPowerLevels, PowerLevelsEventContent},
            tombstone::TombstoneEventContent,
        },
        Algorithm, AnyStrippedStateEvent, AnySyncRoomEvent, AnySyncStateEvent, EventType,
        StrippedStateEvent, SyncStateEvent,
    },
    identifiers::{RoomAliasId, RoomId, UserId},
    js_int::{int, uint, Int, UInt},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, trace};

#[cfg(feature = "messages")]
use super::message::MessageQueue;
use super::RoomMember;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
/// `RoomName` allows the calculation of a text room name.
pub struct RoomName {
    /// The displayed name of the room.
    name: Option<String>,
    /// The canonical alias of the room ex. `#room-name:example.com` and port
    /// number.
    canonical_alias: Option<RoomAliasId>,
    /// List of `RoomAliasId`s the room has been given.
    aliases: Vec<RoomAliasId>,
    /// Users which can be used to generate a room name if the room does not
    /// have one. Required if room name or canonical aliases are not set or
    /// empty.
    pub heroes: Vec<String>,
    /// Number of users whose membership status is `join`. Required if field
    /// has changed since last sync; otherwise, it may be omitted.
    pub joined_member_count: Option<UInt>,
    /// Number of users whose membership status is `invite`. Required if field
    /// has changed since last sync; otherwise, it may be omitted.
    pub invited_member_count: Option<UInt>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerLevels {
    /// The level required to ban a user.
    pub ban: Int,
    /// The level required to send specific event types.
    ///
    /// This is a mapping from event type to power level required.
    pub events: BTreeMap<EventType, Int>,
    /// The default level required to send message events.
    pub events_default: Int,
    /// The level required to invite a user.
    pub invite: Int,
    /// The level required to kick a user.
    pub kick: Int,
    /// The level required to redact an event.
    pub redact: Int,
    /// The default level required to send state events.
    pub state_default: Int,
    /// The default power level for every user in the room.
    pub users_default: Int,
    /// The power level requirements for specific notification types.
    ///
    /// This is a mapping from `key` to power level for that notifications key.
    pub notifications: Int,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
/// Encryption info of the room.
pub struct EncryptionInfo {
    /// The encryption algorithm that should be used to encrypt messages in the
    /// room.
    algorithm: Algorithm,
    /// How long should a session be used before it is rotated.
    rotation_period_ms: u64,
    /// The maximum amount of messages that should be encrypted using the same
    /// session.
    rotation_period_messages: u64,
}

impl EncryptionInfo {
    /// The encryption algorithm that should be used to encrypt messages in the
    /// room.
    pub fn algorithm(&self) -> &Algorithm {
        &self.algorithm
    }

    /// How long should a session be used before it is rotated.
    pub fn rotation_period(&self) -> u64 {
        self.rotation_period_ms
    }

    /// The maximum amount of messages that should be encrypted using the same
    /// session.
    pub fn rotation_period_messages(&self) -> u64 {
        self.rotation_period_messages
    }
}

impl From<&SyncStateEvent<EncryptionEventContent>> for EncryptionInfo {
    fn from(event: &SyncStateEvent<EncryptionEventContent>) -> Self {
        EncryptionInfo {
            algorithm: event.content.algorithm.clone(),
            rotation_period_ms: event
                .content
                .rotation_period_ms
                .map_or(604_800_000, Into::into),
            rotation_period_messages: event.content.rotation_period_msgs.map_or(100, Into::into),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tombstone {
    /// A server-defined message.
    body: String,
    /// The room that is now active.
    replacement: RoomId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
/// A Matrix room.
pub struct Room {
    /// The unique id of the room.
    pub room_id: RoomId,
    /// The name of the room, clients use this to represent a room.
    pub room_name: RoomName,
    /// The mxid of our own user.
    pub own_user_id: UserId,
    /// The mxid of the room creator.
    pub creator: Option<UserId>,
    // TODO: Track banned members, e.g. for /unban support?
    /// The map of invited room members.
    pub invited_members: HashMap<UserId, RoomMember>,
    /// The map of joined room members.
    pub joined_members: HashMap<UserId, RoomMember>,
    /// A queue of messages, holds no more than 10 of the most recent messages.
    ///
    /// This is helpful when using a `StateStore` to avoid multiple requests to
    /// the server for messages.
    #[cfg(feature = "messages")]
    #[cfg_attr(docsrs, doc(cfg(feature = "messages")))]
    #[serde(with = "super::message::ser_deser")]
    pub messages: MessageQueue,
    /// A list of users that are currently typing.
    pub typing_users: Vec<UserId>,
    /// The power level requirements for specific actions in this room
    pub power_levels: Option<PowerLevels>,
    /// Optional encryption info, will be `Some` if the room is encrypted.
    pub encrypted: Option<EncryptionInfo>,
    /// Number of unread notifications with highlight flag set.
    pub unread_highlight: Option<UInt>,
    /// Number of unread notifications.
    pub unread_notifications: Option<UInt>,
    /// The tombstone state of this room.
    pub tombstone: Option<Tombstone>,
}

impl RoomName {
    pub fn push_alias(&mut self, alias: RoomAliasId) -> bool {
        self.aliases.push(alias);
        true
    }

    pub fn set_canonical(&mut self, alias: RoomAliasId) -> bool {
        self.canonical_alias = Some(alias);
        true
    }

    pub fn set_name(&mut self, name: &str) -> bool {
        self.name = Some(name.to_string());
        true
    }

    /// Calculate the canonical display name of the room, taking into account
    /// its name, aliases and members.
    ///
    /// The display name is calculated according to [this algorithm][spec].
    ///
    /// [spec]:
    /// <https://matrix.org/docs/spec/client_server/latest#calculating-the-display-name-for-a-room>
    pub fn calculate_name(
        &self,
        own_user_id: &UserId,
        invited_members: &HashMap<UserId, RoomMember>,
        joined_members: &HashMap<UserId, RoomMember>,
    ) -> String {
        if let Some(name) = &self.name {
            let name = name.trim();
            name.to_string()
        } else if let Some(alias) = &self.canonical_alias {
            let alias = alias.alias().trim();
            alias.to_string()
        } else if !self.aliases.is_empty() && !self.aliases[0].alias().is_empty() {
            self.aliases[0].alias().trim().to_string()
        } else {
            let joined = self.joined_member_count.unwrap_or_else(|| uint!(0));
            let invited = self.invited_member_count.unwrap_or_else(|| uint!(0));
            let heroes = UInt::new(self.heroes.len() as u64).unwrap();
            let invited_joined = (invited + joined).saturating_sub(uint!(1));

            let members = joined_members.values().chain(invited_members.values());

            // TODO: This should use `self.heroes` but it is always empty??
            if heroes >= invited_joined {
                let mut names = members
                    .filter(|m| m.user_id != *own_user_id)
                    .take(3)
                    .map(|mem| {
                        mem.display_name
                            .clone()
                            .unwrap_or_else(|| mem.user_id.localpart().to_string())
                    })
                    .collect::<Vec<String>>();
                // stabilize ordering
                names.sort();
                names.join(", ")
            } else if heroes < invited_joined && invited + joined > uint!(1) {
                let mut names = members
                    .filter(|m| m.user_id != *own_user_id)
                    .take(3)
                    .map(|mem| {
                        mem.display_name
                            .clone()
                            .unwrap_or_else(|| mem.user_id.localpart().to_string())
                    })
                    .collect::<Vec<String>>();
                names.sort();

                // TODO: What length does the spec want us to use here and in
                // the `else`?
                format!("{}, and {} others", names.join(", "), (joined + invited))
            } else {
                "Empty room".to_string()
            }
        }
    }
}

impl Room {
    /// Create a new room.
    ///
    /// # Arguments
    ///
    /// * `room_id` - The unique id of the room.
    ///
    /// * `own_user_id` - The mxid of our own user.
    pub fn new(room_id: &RoomId, own_user_id: &UserId) -> Self {
        Room {
            room_id: room_id.clone(),
            room_name: RoomName::default(),
            own_user_id: own_user_id.clone(),
            creator: None,
            invited_members: HashMap::new(),
            joined_members: HashMap::new(),
            #[cfg(feature = "messages")]
            messages: MessageQueue::new(),
            typing_users: Vec::new(),
            power_levels: None,
            encrypted: None,
            unread_highlight: None,
            unread_notifications: None,
            tombstone: None,
        }
    }

    /// Return the display name of the room.
    pub fn display_name(&self) -> String {
        self.room_name.calculate_name(
            &self.own_user_id,
            &self.invited_members,
            &self.joined_members,
        )
    }

    /// Is the room a encrypted room.
    pub fn is_encrypted(&self) -> bool {
        self.encrypted.is_some()
    }

    /// Get the encryption info if any of the room.
    ///
    /// Returns None if the room is not encrypted.
    pub fn encryption_info(&self) -> Option<&EncryptionInfo> {
        self.encrypted.as_ref()
    }

    /// Process the join or invite event for a new room member.
    ///
    /// This method should only be called on events which add new members, not
    /// those related to existing ones.
    ///
    /// Returns a tuple of:
    ///
    /// 1. True if the event made changes to the room's state, false otherwise.
    /// 2. A map of display name ambiguity status changes (see
    ///    `disambiguation_updates`).
    ///
    /// # Arguments
    ///
    /// * `target_member` - The ID of the member to add.
    /// * `event` - The join or invite event for the specified room member.
    fn add_member(
        &mut self,
        target_member: &UserId,
        event: &SyncStateEvent<MemberEventContent>,
    ) -> (bool, HashMap<UserId, bool>) {
        let new_member = RoomMember::new(event, &self.room_id);

        if self.joined_members.contains_key(&new_member.user_id) {
            error!("add_member called on event of an already joined user");
            return (false, HashMap::new());
        }

        // Perform display name disambiguations, if necessary.
        let disambiguations =
            self.disambiguation_updates(target_member, None, new_member.display_name.clone());

        debug!("add_member: disambiguations: {:#?}", disambiguations);

        match event.content.membership {
            MembershipState::Join => {
                // Since the member is now joined, he shouldn't be tracked as an invited member any
                // longer if he was previously tracked as such.
                self.invited_members.remove(target_member);

                self.joined_members
                    .insert(target_member.clone(), new_member)
            }

            MembershipState::Invite => self
                .invited_members
                .insert(target_member.clone(), new_member),

            _ => panic!("Room::add_member called on event that is neither `join` nor `invite`."),
        };

        for (id, is_ambiguous) in disambiguations.iter() {
            self.get_member_mut(id).unwrap().display_name_ambiguous = *is_ambiguous;
        }

        (true, disambiguations)
    }

    /// Process the leaving event for a room member.
    ///
    /// Returns a tuple of:
    ///
    /// 1. True if the event made changes to the room's state, false otherwise.
    /// 2. A map of display name ambiguity status changes (see
    ///    `disambiguation_updates`).
    ///
    /// # Arguments
    ///
    /// * `target_member` - The ID of the member to remove.
    /// * `event` - The leaving event for the specified room member.
    fn remove_member(
        &mut self,
        target_member: &UserId,
        event: &SyncStateEvent<MemberEventContent>,
    ) -> (bool, HashMap<UserId, bool>) {
        let leaving_member = RoomMember::new(event, &self.room_id);

        if self.get_member(target_member).is_none() {
            return (false, HashMap::new());
        }

        // Perform display name disambiguations, if necessary.
        let disambiguations =
            self.disambiguation_updates(target_member, leaving_member.display_name, None);

        debug!("remove_member: disambiguations: {:#?}", disambiguations);

        for (id, is_ambiguous) in disambiguations.iter() {
            self.get_member_mut(id).unwrap().display_name_ambiguous = *is_ambiguous;
        }

        // TODO: factor this out to a method called `remove_member` and rename this method
        // to something like `process_member_leaving_event`.
        self.joined_members
            .remove(target_member)
            .or_else(|| self.invited_members.remove(target_member));

        (true, disambiguations)
    }

    /// Check whether the user with the MXID `user_id` is joined or invited to
    /// the room.
    ///
    /// Returns true if so, false otherwise.
    pub fn member_is_tracked(&self, user_id: &UserId) -> bool {
        self.invited_members.contains_key(&user_id) || self.joined_members.contains_key(&user_id)
    }

    /// Get a room member by user ID.
    ///
    /// If there is no such member, returns `None`.
    pub fn get_member(&self, user_id: &UserId) -> Option<&RoomMember> {
        self.joined_members
            .get(user_id)
            .or_else(|| self.invited_members.get(user_id))
    }

    /// Get a room member by user ID.
    ///
    /// If there is no such member, returns `None`.
    pub fn get_member_mut(&mut self, user_id: &UserId) -> Option<&mut RoomMember> {
        match self.joined_members.get_mut(user_id) {
            None => self.invited_members.get_mut(user_id),
            Some(m) => Some(m),
        }
    }

    /// Given a display name, return the set of members which share it.
    fn display_name_equivalence_set(&self, name: &str) -> HashSet<UserId> {
        let members = self
            .invited_members
            .iter()
            .chain(self.joined_members.iter());

        // Find all other users that share the display name with the joining user.
        members
            .filter(|(_, member)| {
                member
                    .display_name
                    .as_ref()
                    .map(|other_name| other_name == name)
                    .unwrap_or(false)
            })
            .map(|(_, member)| member.user_id.clone())
            .collect()
    }

    /// Answers the question "If `member` changed their display name from
    /// `old_name` to `new_name`, which members' display names would become
    /// ambiguous and which would no longer be ambiguous?".
    ///
    /// Returns the map of ambiguity status changes for those members which
    /// would be affected by the change.
    ///
    /// It is important that this method be called *before* any changes are made
    /// to the model, i.e. before any actual display name changes.
    ///
    /// # Arguments
    ///
    /// - `member`: The MXID of the member who is changing their display name.
    /// - `old_name`: The old display name of `member`. May be `None` if
    ///   `member` had no display name in the room before (because he had not
    ///   set it or he is just entering the room).
    /// - `new_name`: The new display name of `member`. May be `None` if
    ///   `member` will no longer have a display name in the room after the
    ///   change (because he is removing it or exiting the room).
    fn disambiguation_updates(
        &self,
        member: &UserId,
        old_name: Option<String>,
        new_name: Option<String>,
    ) -> HashMap<UserId, bool> {
        let old_name_eq_set = match old_name {
            None => HashSet::new(),
            Some(name) => self.display_name_equivalence_set(&name),
        };

        let disambiguate_old = match old_name_eq_set.len().saturating_sub(1) {
            n if n > 1 => vec![(member.clone(), false)].into_iter().collect(),
            1 => old_name_eq_set.into_iter().map(|m| (m, false)).collect(),
            0 => HashMap::new(),
            _ => panic!("impossible"),
        };

        //

        let mut new_name_eq_set = match new_name {
            None => HashSet::new(),
            Some(name) => self.display_name_equivalence_set(&name),
        };

        new_name_eq_set.insert(member.clone());

        let disambiguate_new = match new_name_eq_set.len() {
            1 => HashMap::new(),
            2 => new_name_eq_set.into_iter().map(|m| (m, true)).collect(),
            _ => vec![(member.clone(), true)].into_iter().collect(),
        };

        disambiguate_old
            .into_iter()
            .chain(disambiguate_new.into_iter())
            .collect()
    }

    /// Add to the list of `RoomAliasId`s.
    fn push_room_alias(&mut self, alias: &RoomAliasId) -> bool {
        self.room_name.push_alias(alias.clone());
        true
    }

    /// RoomAliasId is `#alias:hostname` and `port`
    fn canonical_alias(&mut self, alias: &RoomAliasId) -> bool {
        self.room_name.set_canonical(alias.clone());
        true
    }

    fn set_room_name(&mut self, name: &str) -> bool {
        self.room_name.set_name(name);
        true
    }

    fn set_room_power_level(&mut self, event: &SyncStateEvent<PowerLevelsEventContent>) -> bool {
        let PowerLevelsEventContent {
            ban,
            events,
            events_default,
            invite,
            kick,
            redact,
            state_default,
            users_default,
            notifications: NotificationPowerLevels { room },
            ..
        } = &event.content;

        let power = PowerLevels {
            ban: *ban,
            events: events.clone(),
            events_default: *events_default,
            invite: *invite,
            kick: *kick,
            redact: *redact,
            state_default: *state_default,
            users_default: *users_default,
            notifications: *room,
        };
        self.power_levels = Some(power);
        true
    }

    pub(crate) fn set_room_summary(&mut self, summary: &RoomSummary) {
        let RoomSummary {
            heroes,
            joined_member_count,
            invited_member_count,
        } = summary;
        self.room_name.heroes = heroes.clone();
        self.room_name.invited_member_count = *invited_member_count;
        self.room_name.joined_member_count = *joined_member_count;
    }

    pub(crate) fn set_unread_notice_count(&mut self, notifications: &UnreadNotificationsCount) {
        self.unread_highlight = notifications.highlight_count;
        self.unread_notifications = notifications.notification_count;
    }

    /// Handle a room.member updating the room state if necessary.
    ///
    /// Returns a tuple of:
    ///
    /// 1. True if the joined member list changed, false otherwise.
    /// 2. A map of display name ambiguity status changes (see
    ///    `disambiguation_updates`).
    pub fn handle_membership(
        &mut self,
        event: &SyncStateEvent<MemberEventContent>,
        state_event: bool,
    ) -> (bool, HashMap<UserId, bool>) {
        use MembershipChange::*;
        use MembershipState::*;

        trace!(
            "Received {} event: {}",
            if state_event { "state" } else { "timeline" },
            event.event_id
        );

        let target_user = match UserId::try_from(event.state_key.clone()) {
            Ok(id) => id,
            Err(e) => {
                error!("Received a member event with invalid state_key: {}", e);
                return (false, HashMap::new());
            }
        };

        if state_event && !self.member_is_tracked(&target_user) {
            debug!(
                "handle_membership: User {user_id} is {state} the room {room_id} ({room_name})",
                user_id = target_user,
                state = event.content.membership.describe(),
                room_id = self.room_id,
                room_name = self.display_name(),
            );

            match event.content.membership {
                Join | Invite => self.add_member(&target_user, event),

                // We are not interested in tracking past members for now
                _ => (false, HashMap::new()),
            }
        } else {
            let change = event.membership_change();

            debug!(
                "handle_membership: User {user_id} {action} the room {room_id} ({room_name})",
                user_id = target_user,
                action = change.describe(),
                room_id = self.room_id,
                room_name = self.display_name(),
            );

            match change {
                Invited | Joined => self.add_member(&target_user, event),
                Kicked | Banned | KickedAndBanned | InvitationRejected | Left => {
                    self.remove_member(&target_user, event)
                }
                ProfileChanged { .. } => self.update_member_profile(&target_user, event, change),

                // Not interested in other events.
                _ => (false, HashMap::new()),
            }
        }
    }

    /// Handle a room.message event and update the `MessageQueue` if necessary.
    ///
    /// Returns true if `MessageQueue` was added to.
    #[cfg(feature = "messages")]
    #[cfg_attr(docsrs, doc(cfg(feature = "messages")))]
    pub fn handle_message(&mut self, event: &AnySyncMessageEvent) -> bool {
        self.messages
            .push(AnyPossiblyRedactedSyncMessageEvent::Regular(event.clone()))
    }

    /// Handle a room.redaction event and update the `MessageQueue`.
    ///
    /// Returns true if `MessageQueue` was updated. The effected message event
    /// has it's contents replaced with the `RedactionEventContents` and the
    /// whole redaction event is added to the `Unsigned` `redacted_because`
    /// field.
    #[cfg(feature = "messages")]
    #[cfg_attr(docsrs, doc(cfg(feature = "messages")))]
    pub fn handle_redaction(&mut self, redacted_event: &SyncRedactionEvent) -> bool {
        use crate::{identifiers::RoomVersionId, models::message::PossiblyRedactedExt};

        if let Some(mut msg) = self
            .messages
            .iter_mut()
            .find(|msg| &redacted_event.redacts == msg.event_id())
        {
            match msg.deref_mut() {
                AnyPossiblyRedactedSyncMessageEvent::Regular(event) => {
                    *msg = AnyPossiblyRedactedSyncMessageEvent::Redacted(
                        event
                            .clone()
                            .redact(redacted_event.clone(), RoomVersionId::Version6),
                    );
                }
                AnyPossiblyRedactedSyncMessageEvent::Redacted(_) => return false,
            }
            true
        } else {
            false
        }
    }

    /// Handle a room.aliases event, updating the room state if necessary.
    ///
    /// Returns true if the room name changed, false otherwise.
    pub fn handle_room_aliases(&mut self, event: &SyncStateEvent<AliasesEventContent>) -> bool {
        match event.content.aliases.as_slice() {
            [alias] => self.push_room_alias(alias),
            [alias, ..] => self.push_room_alias(alias),
            _ => false,
        }
    }

    /// Handle a room.canonical_alias event, updating the room state if
    /// necessary.
    ///
    /// Returns true if the room name changed, false otherwise.
    pub fn handle_canonical(&mut self, event: &SyncStateEvent<CanonicalAliasEventContent>) -> bool {
        match &event.content.alias {
            Some(name) => self.canonical_alias(&name),
            _ => false,
        }
    }

    /// Handle a room.name event, updating the room state if necessary.
    ///
    /// Returns true if the room name changed, false otherwise.
    pub fn handle_room_name(&mut self, event: &SyncStateEvent<NameEventContent>) -> bool {
        match event.content.name() {
            Some(name) => self.set_room_name(name),
            _ => false,
        }
    }

    /// Handle a room.name event, updating the room state if necessary.
    ///
    /// Returns true if the room name changed, false otherwise.
    pub fn handle_stripped_room_name(
        &mut self,
        event: &StrippedStateEvent<NameEventContent>,
    ) -> bool {
        match event.content.name() {
            Some(name) => self.set_room_name(name),
            _ => false,
        }
    }

    /// Handle a room.power_levels event, updating the room state if necessary.
    ///
    /// Returns true if the room name changed, false otherwise.
    pub fn handle_power_level(&mut self, event: &SyncStateEvent<PowerLevelsEventContent>) -> bool {
        // NOTE: this is always true, we assume that if we get an event their is an update.
        let mut updated = self.set_room_power_level(event);

        let mut max_power = event.content.users_default;
        for power in event.content.users.values() {
            max_power = *power.max(&max_power);
        }

        for user in event.content.users.keys() {
            if let Some(member) = self.joined_members.get_mut(user) {
                if Room::update_member_power(member, event, max_power) {
                    updated = true;
                }
            }
        }

        updated
    }

    fn handle_tombstone(&mut self, event: &SyncStateEvent<TombstoneEventContent>) -> bool {
        self.tombstone = Some(Tombstone {
            body: event.content.body.clone(),
            replacement: event.content.replacement_room.clone(),
        });
        true
    }

    fn handle_encryption_event(&mut self, event: &SyncStateEvent<EncryptionEventContent>) -> bool {
        self.encrypted = Some(event.into());
        true
    }

    /// Receive a timeline event for this room and update the room state.
    ///
    /// Returns true if the joined member list changed, false otherwise.
    ///
    /// # Arguments
    ///
    /// * `event` - The event of the room.
    pub fn receive_timeline_event(&mut self, event: &AnySyncRoomEvent) -> bool {
        match event {
            AnySyncRoomEvent::State(event) => match event {
                // update to the current members of the room
                AnySyncStateEvent::RoomMember(event) => self.handle_membership(event, false).0,
                // finds all events related to the name of the room for later use
                AnySyncStateEvent::RoomName(event) => self.handle_room_name(event),
                AnySyncStateEvent::RoomCanonicalAlias(event) => self.handle_canonical(event),
                AnySyncStateEvent::RoomAliases(event) => self.handle_room_aliases(event),
                // power levels of the room members
                AnySyncStateEvent::RoomPowerLevels(event) => self.handle_power_level(event),
                AnySyncStateEvent::RoomTombstone(event) => self.handle_tombstone(event),
                AnySyncStateEvent::RoomEncryption(event) => self.handle_encryption_event(event),
                _ => false,
            },
            AnySyncRoomEvent::Message(event) => match event {
                #[cfg(feature = "messages")]
                // We ignore this variants event because `handle_message` takes the enum
                // to store AnySyncMessageEvent events in the `MessageQueue`.
                AnySyncMessageEvent::RoomMessage(_) => self.handle_message(event),
                #[cfg(feature = "messages")]
                AnySyncMessageEvent::RoomRedaction(event) => self.handle_redaction(event),
                _ => false,
            },
            AnySyncRoomEvent::RedactedMessage(_) | AnySyncRoomEvent::RedactedState(_) => false,
        }
    }

    /// Receive a state event for this room and update the room state.
    ///
    /// Returns true if the state of the `Room` has changed, false otherwise.
    ///
    /// # Arguments
    ///
    /// * `event` - The event of the room.
    pub fn receive_state_event(&mut self, event: &AnySyncStateEvent) -> bool {
        match event {
            // update to the current members of the room
            AnySyncStateEvent::RoomMember(member) => self.handle_membership(member, true).0,
            // finds all events related to the name of the room for later use
            AnySyncStateEvent::RoomName(name) => self.handle_room_name(name),
            AnySyncStateEvent::RoomCanonicalAlias(c_alias) => self.handle_canonical(c_alias),
            AnySyncStateEvent::RoomAliases(alias) => self.handle_room_aliases(alias),
            // power levels of the room members
            AnySyncStateEvent::RoomPowerLevels(power) => self.handle_power_level(power),
            AnySyncStateEvent::RoomTombstone(tomb) => self.handle_tombstone(tomb),
            AnySyncStateEvent::RoomEncryption(encrypt) => self.handle_encryption_event(encrypt),
            _ => false,
        }
    }

    /// Receive a stripped state event for this room and update the room state.
    ///
    /// Returns true if the state of the `Room` has changed, false otherwise.
    ///
    /// # Arguments
    ///
    /// * `event` - The `AnyStrippedStateEvent` sent by the server for invited
    ///   but not joined rooms.
    pub fn receive_stripped_state_event(&mut self, event: &AnyStrippedStateEvent) -> bool {
        match event {
            AnyStrippedStateEvent::RoomName(event) => self.handle_stripped_room_name(event),
            _ => false,
        }
    }

    /// Receive a presence event for a member of the current room.
    ///
    /// Returns true if the event causes a change to the member's presence,
    /// false otherwise.
    ///
    /// # Arguments
    ///
    /// * `event` - The presence event to receive and process.
    pub fn receive_presence_event(&mut self, event: &PresenceEvent) -> bool {
        let PresenceEvent {
            content:
                PresenceEventContent {
                    avatar_url,
                    currently_active,
                    displayname,
                    last_active_ago,
                    presence,
                    status_msg,
                },
            ..
        } = event;

        if let Some(member) = self.joined_members.get_mut(&event.sender) {
            if member.display_name == *displayname
                && member.avatar_url == *avatar_url
                && member.presence.as_ref() == Some(presence)
                && member.status_msg == *status_msg
                && member.last_active_ago == *last_active_ago
                && member.currently_active == *currently_active
            {
                // Everything is the same, nothing to do.
                false
            } else {
                // Something changed, do the update.

                member.presence_events.push(event.clone());
                member.avatar_url = avatar_url.clone();
                member.currently_active = *currently_active;
                member.display_name = displayname.clone();
                member.last_active_ago = *last_active_ago;
                member.presence = Some(*presence);
                member.status_msg = status_msg.clone();

                true
            }
        } else {
            // This is probably an error as we have a `PresenceEvent` for a user
            // we don't know about.
            false
        }
    }

    /// Process an update of a member's profile.
    ///
    /// Returns a tuple of:
    ///
    /// 1. True if the event made changes to the room's state, false otherwise.
    /// 2. A map of display name ambiguity status changes (see
    ///    `disambiguation_updates`).
    ///
    /// # Arguments
    ///
    /// * `target_member` - The ID of the member to update.
    /// * `event` - The profile update event for the specified room member.
    pub fn update_member_profile(
        &mut self,
        target_member: &UserId,
        event: &SyncStateEvent<MemberEventContent>,
        change: MembershipChange,
    ) -> (bool, HashMap<UserId, bool>) {
        let member = self.get_member(target_member);
        let member = match member {
            Some(member) => member,

            None => {
                debug!("update_member_profile [{}]: Got a profile update for user {} but he's not a room member",
                    self.room_id, target_member);
                return (false, HashMap::new());
            }
        };

        let old_name = member.display_name.clone();
        let new_name = event.content.displayname.clone();

        match change {
            MembershipChange::ProfileChanged {
                displayname_changed,
                avatar_url_changed,
            } => {
                if displayname_changed {
                    debug!(
                        "update_member_profile [{}]: {} changed display name from {:#?} to {:#?}",
                        self.room_id, target_member, old_name, &new_name
                    );
                }

                if avatar_url_changed {
                    debug!(
                        "update_member_profile [{}]: {} changed avatar URL from {:#?} to {:#?}",
                        self.room_id, target_member, &member.avatar_url, &new_name
                    );
                }
            }

            _ => {
                error!(
                    "update_member_profile [{}]: got a ProfileChanged but nothing changed",
                    self.room_id
                );
                return (false, HashMap::new());
            }
        }

        let disambiguations =
            self.disambiguation_updates(target_member, old_name, new_name.clone());
        for (id, is_ambiguous) in disambiguations.iter() {
            if self.get_member_mut(id).is_none() {
                debug!("update_member_profile [{}]: Tried disambiguating display name for {} but he's not there",
                    self.room_id,
                    id);
            } else {
                self.get_member_mut(id).unwrap().display_name_ambiguous = *is_ambiguous;
            }
        }

        debug!(
            "update_member_profile [{}]: disambiguations: {:#?}",
            self.room_id, &disambiguations
        );

        let changed = match self.get_member_mut(target_member) {
            Some(member) => {
                member.display_name = new_name;
                member.avatar_url = event.content.avatar_url.clone();
                true
            }
            None => {
                error!(
                    "update_member_profile [{}]: user {} does not exist",
                    self.room_id, target_member
                );

                false
            }
        };

        (changed, disambiguations)
    }

    /// Process an update of a member's power level.
    ///
    /// # Arguments
    ///
    /// * `event` - The power level event to process.
    /// * `max_power` - Maximum power level allowed.
    pub fn update_member_power(
        member: &mut RoomMember,
        event: &SyncStateEvent<PowerLevelsEventContent>,
        max_power: Int,
    ) -> bool {
        let changed;

        if let Some(user_power) = event.content.users.get(&member.user_id) {
            changed = member.power_level != Some(*user_power);
            member.power_level = Some(*user_power);
        } else {
            changed = member.power_level != Some(event.content.users_default);
            member.power_level = Some(event.content.users_default);
        }

        if max_power > int!(0) {
            member.power_level_norm = Some((member.power_level.unwrap() * int!(100)) / max_power);
        }

        changed
    }
}

trait Describe {
    fn describe(&self) -> String;
}

impl Describe for MembershipState {
    fn describe(&self) -> String {
        match self {
            Self::Ban => "is banned in",
            Self::Invite => "is invited to",
            Self::Join => "is a member of",
            Self::Knock => "is requesting access",
            Self::Leave => "left",
            _ => "unhandled case of MembershipState",
        }
        .to_string()
    }
}

impl Describe for MembershipChange {
    fn describe(&self) -> String {
        match self {
            Self::Invited => "got invited to",
            Self::Joined => "joined",
            Self::Kicked => "got kicked from",
            Self::Banned => "got banned from",
            Self::Unbanned => "got unbanned from",
            Self::KickedAndBanned => "got kicked and banned from",
            Self::InvitationRejected => "rejected the invitation to",
            Self::InvitationRevoked => "got their invitation revoked from",
            Self::Left => "left",
            Self::ProfileChanged {
                displayname_changed,
                avatar_url_changed,
            } => match (*displayname_changed, *avatar_url_changed) {
                (true, true) => "changed their displayname and avatar",
                (true, false) => "changed their displayname",
                (false, true) => "changed their avatar",
                _ => {
                    error!("Got ProfileChanged but nothing changed");
                    "impossible: changed nothing in their profile"
                }
            },
            Self::None => "did nothing in",
            Self::NotImplemented => "NOT IMPLEMENTED",
            Self::Error => "ERROR",
            _ => "unhandled case of MembershipChange",
        }
        .to_string()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    use crate::{
        events::{room::encryption::EncryptionEventContent, Unsigned},
        identifiers::EventId,
        Raw,
    };
    use crate::{identifiers::UserId, BaseClient, Session};
    use matrix_sdk_test::{async_test, sync_response, EventBuilder, EventsJson, SyncResponseFile};

    #[cfg(not(target_arch = "wasm32"))]
    use std::time::SystemTime;

    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::*;

    use std::{convert::TryFrom, ops::Deref};

    async fn get_client() -> BaseClient {
        let session = Session {
            access_token: "1234".to_owned(),
            user_id: UserId::try_from("@example:localhost").unwrap(),
            device_id: "DEVICEID".into(),
        };
        let client = BaseClient::new().unwrap();
        client.restore_login(session).await.unwrap();
        client
    }

    fn get_room_id() -> RoomId {
        RoomId::try_from("!SVkFJHzfwvuaIEawgC:localhost").unwrap()
    }

    #[async_test]
    async fn user_presence() {
        let client = get_client().await;

        let mut response = sync_response(SyncResponseFile::Default);

        client.receive_sync_response(&mut response).await.unwrap();

        let rooms_lock = &client.joined_rooms();
        let rooms = rooms_lock.read().await;
        let room = &rooms
            .get(&RoomId::try_from("!SVkFJHzfwvuaIEawgC:localhost").unwrap())
            .unwrap()
            .read()
            .await;

        assert_eq!(1, room.joined_members.len());
        assert!(room.deref().power_levels.is_some())
    }

    #[cfg(feature = "messages")]
    #[test]
    fn message_edit_deser() {
        let json = matrix_sdk_test::test_json::MESSAGE_EDIT.deref();
        let event = serde_json::from_value::<Raw<AnySyncMessageEvent>>(json.clone()).unwrap();

        if let Ok(AnySyncMessageEvent::RoomMessage(ev)) = event.deserialize() {
            if let matrix_sdk_common::events::room::message::MessageEventContent::Text(content) =
                ev.content
            {
                assert_eq!(content.body, " * edited message");
                assert!(content.relates_to.is_some());
            }
        } else {
            panic!("{:?}", event);
        }
    }

    #[async_test]
    async fn member_is_not_both_invited_and_joined() {
        let client = get_client().await;
        let room_id = get_room_id();
        let user_id1 = UserId::try_from("@example:localhost").unwrap();
        let user_id2 = UserId::try_from("@example2:localhost").unwrap();

        let member2_invite_event = serde_json::json!({
            "content": {
                "avatar_url": null,
                "displayname": "example2",
                "membership": "invite"
            },
            "event_id": "$16345217l517tabbz:localhost",
            "membership": "join",
            "origin_server_ts": 1455123234,
            "sender": format!("{}", user_id1),
            "state_key": format!("{}", user_id2),
            "type": "m.room.member",
            "unsigned": {
                "age": 1989321234,
                "replaces_state": "$1622a2311315tkjoA:localhost"
            }
        });

        let member2_join_event = serde_json::json!({
            "content": {
                "avatar_url": null,
                "displayname": "example2",
                "membership": "join"
            },
            "event_id": "$163409224327jkbba:localhost",
            "membership": "join",
            "origin_server_ts": 1455123238,
            "sender": format!("{}", user_id2),
            "state_key": format!("{}", user_id2),
            "type": "m.room.member",
            "prev_content": {
                "avatar_url": null,
                "displayname": "example2",
                "membership": "invite"
            },
            "unsigned": {
                "age": 1989321214,
                "replaces_state": "$16345217l517tabbz:localhost"
            }
        });

        let mut event_builder = EventBuilder::new();

        let mut member1_join_sync_response = event_builder
            .add_room_event(EventsJson::Member)
            .build_sync_response();

        let mut member2_invite_sync_response = event_builder
            .add_custom_joined_event(&room_id, member2_invite_event)
            .build_sync_response();

        let mut member2_join_sync_response = event_builder
            .add_custom_joined_event(&room_id, member2_join_event)
            .build_sync_response();

        // Test that `user` is either joined or invited to `room` but not both.
        async fn invited_or_joined_but_not_both(client: &BaseClient, room: &RoomId, user: &UserId) {
            let room = client.get_joined_room(&room).await.unwrap();
            let room = room.read().await;

            assert!(
                room.invited_members.get(&user).is_none()
                    || room.joined_members.get(&user).is_none()
            );
            assert!(
                room.invited_members.get(&user).is_some()
                    || room.joined_members.get(&user).is_some()
            );
        };

        // First member joins.
        client
            .receive_sync_response(&mut member1_join_sync_response)
            .await
            .unwrap();

        // The first member is not *both* invited and joined but it *is* one of those.
        invited_or_joined_but_not_both(&client, &room_id, &user_id1).await;

        // First member invites second member.
        client
            .receive_sync_response(&mut member2_invite_sync_response)
            .await
            .unwrap();

        // Neither member is *both* invited and joined, but they are both *at least one* of those.
        invited_or_joined_but_not_both(&client, &room_id, &user_id1).await;
        invited_or_joined_but_not_both(&client, &room_id, &user_id2).await;

        // Second member joins.
        client
            .receive_sync_response(&mut member2_join_sync_response)
            .await
            .unwrap();

        // Repeat the previous test.
        invited_or_joined_but_not_both(&client, &room_id, &user_id1).await;
        invited_or_joined_but_not_both(&client, &room_id, &user_id2).await;
    }

    #[async_test]
    async fn test_member_display_name() {
        // Initialize

        let client = get_client().await;
        let room_id = get_room_id();
        let user_id1 = UserId::try_from("@example:localhost").unwrap();
        let user_id2 = UserId::try_from("@example2:localhost").unwrap();
        let user_id3 = UserId::try_from("@example3:localhost").unwrap();

        let member2_join_event = serde_json::json!({
            "content": {
                "avatar_url": null,
                "displayname": "example",
                "membership": "join"
            },
            "event_id": "$16345217l517tabbz:localhost",
            "membership": "join",
            "origin_server_ts": 1455123234,
            "sender": format!("{}", user_id2),
            "state_key": format!("{}", user_id2),
            "type": "m.room.member",
            "prev_content": {
                "avatar_url": null,
                "displayname": "example",
                "membership": "invite"
            },
            "unsigned": {
                "age": 1989321234,
                "replaces_state": "$1622a2311315tkjoA:localhost"
            }
        });

        let member1_invites_member2_event = serde_json::json!({
            "content": {
                "avatar_url": null,
                "displayname": "example",
                "membership": "invite"
            },
            "event_id": "$16345217l517tabbz:localhost",
            "membership": "invite",
            "origin_server_ts": 1455123238,
            "sender": format!("{}", user_id1),
            "state_key": format!("{}", user_id2),
            "type": "m.room.member",
            "unsigned": {
                "age": 1989321238,
                "replaces_state": "$1622a2311315tkjoA:localhost"
            }
        });

        let member2_name_change_event = serde_json::json!({
            "content": {
                "avatar_url": null,
                "displayname": "changed",
                "membership": "join"
            },
            "event_id": "$16345217l517tabbz:localhost",
            "membership": "join",
            "origin_server_ts": 1455123238,
            "sender": format!("{}", user_id2),
            "state_key": format!("{}", user_id2),
            "type": "m.room.member",
            "prev_content": {
                "avatar_url": null,
                "displayname": "example",
                "membership": "join"
            },
            "unsigned": {
                "age": 1989321238,
                "replaces_state": "$1622a2311315tkjoA:localhost"
            }
        });

        let member2_leave_event = serde_json::json!({
            "content": {
                "avatar_url": null,
                "displayname": "example",
                "membership": "leave"
            },
            "event_id": "$263452333l22bggbz:localhost",
            "membership": "leave",
            "origin_server_ts": 1455123228,
            "sender": format!("{}", user_id2),
            "state_key": format!("{}", user_id2),
            "type": "m.room.member",
            "prev_content": {
                "avatar_url": null,
                "displayname": "example",
                "membership": "join"
            },
            "unsigned": {
                "age": 1989321221,
                "replaces_state": "$16345217l517tabbz:localhost"
            }
        });

        let member3_join_event = serde_json::json!({
            "content": {
                "avatar_url": null,
                "displayname": "example",
                "membership": "join"
            },
            "event_id": "$16845287981ktggba:localhost",
            "membership": "join",
            "origin_server_ts": 1455123244,
            "sender": format!("{}", user_id3),
            "state_key": format!("{}", user_id3),
            "type": "m.room.member",
            "prev_content": {
                "avatar_url": null,
                "displayname": "example",
                "membership": "invite"
            },
            "unsigned": {
                "age": 1989321254,
                "replaces_state": "$1622l2323445kabrA:localhost"
            }
        });

        let member3_leave_event = serde_json::json!({
            "content": {
                "avatar_url": null,
                "displayname": "example",
                "membership": "leave"
            },
            "event_id": "$11121987981abfgr:localhost",
            "membership": "leave",
            "origin_server_ts": 1455123230,
            "sender": format!("{}", user_id3),
            "state_key": format!("{}", user_id3),
            "type": "m.room.member",
            "prev_content": {
                "avatar_url": null,
                "displayname": "example",
                "membership": "join"
            },
            "unsigned": {
                "age": 1989321244,
                "replaces_state": "$16845287981ktggba:localhost"
            }
        });

        let mut event_builder = EventBuilder::new();

        let mut member1_join_sync_response = event_builder
            .add_room_event(EventsJson::Member)
            .build_sync_response();

        let mut member2_join_sync_response = event_builder
            .add_custom_joined_event(&room_id, member2_join_event.clone())
            .build_sync_response();

        let mut member3_join_sync_response = event_builder
            .add_custom_joined_event(&room_id, member3_join_event)
            .build_sync_response();

        let mut member2_and_member3_leave_sync_response = event_builder
            .add_custom_joined_event(&room_id, member2_leave_event)
            .add_custom_joined_event(&room_id, member3_leave_event)
            .build_sync_response();

        let mut member2_rejoins_when_invited_sync_response = event_builder
            .add_custom_joined_event(&room_id, member1_invites_member2_event)
            .add_custom_joined_event(&room_id, member2_join_event)
            .build_sync_response();

        let mut member1_name_change_sync_response = event_builder
            .add_room_event(EventsJson::MemberNameChange)
            .build_sync_response();

        let mut member2_name_change_sync_response = event_builder
            .add_custom_joined_event(&room_id, member2_name_change_event)
            .build_sync_response();

        // First member with display name "example" joins
        client
            .receive_sync_response(&mut member1_join_sync_response)
            .await
            .unwrap();

        // First member's disambiguated display name is "example"
        {
            let room = client.get_joined_room(&room_id).await.unwrap();
            let room = room.read().await;
            let display_name1 = room.get_member(&user_id1).unwrap().disambiguated_name();

            assert_eq!("example", display_name1);
        }

        // Second and third member with display name "example" join
        client
            .receive_sync_response(&mut member2_join_sync_response)
            .await
            .unwrap();
        client
            .receive_sync_response(&mut member3_join_sync_response)
            .await
            .unwrap();

        // All of their display names are now disambiguated with MXIDs
        {
            let room = client.get_joined_room(&room_id).await.unwrap();
            let room = room.read().await;
            let display_name1 = room.get_member(&user_id1).unwrap().disambiguated_name();
            let display_name2 = room.get_member(&user_id2).unwrap().disambiguated_name();
            let display_name3 = room.get_member(&user_id3).unwrap().disambiguated_name();

            assert_eq!(format!("example ({})", user_id1), display_name1);
            assert_eq!(format!("example ({})", user_id2), display_name2);
            assert_eq!(format!("example ({})", user_id3), display_name3);
        }

        // Second and third member leave. The first's display name is now just "example" again.
        client
            .receive_sync_response(&mut member2_and_member3_leave_sync_response)
            .await
            .unwrap();

        {
            let room = client.get_joined_room(&room_id).await.unwrap();
            let room = room.read().await;

            let display_name1 = room.get_member(&user_id1).unwrap().disambiguated_name();

            assert_eq!("example", display_name1);
        }

        // Second member rejoins after being invited by first member. Both of their names are
        // disambiguated.
        client
            .receive_sync_response(&mut member2_rejoins_when_invited_sync_response)
            .await
            .unwrap();

        {
            let room = client.get_joined_room(&room_id).await.unwrap();
            let room = room.read().await;

            let display_name1 = room.get_member(&user_id1).unwrap().disambiguated_name();
            let display_name2 = room.get_member(&user_id2).unwrap().disambiguated_name();

            assert_eq!(format!("example ({})", user_id1), display_name1);
            assert_eq!(format!("example ({})", user_id2), display_name2);
        }

        // First member changes his display name to "changed". None of the display names are
        // disambiguated.
        client
            .receive_sync_response(&mut member1_name_change_sync_response)
            .await
            .unwrap();

        {
            let room = client.get_joined_room(&room_id).await.unwrap();
            let room = room.read().await;

            let display_name1 = room.get_member(&user_id1).unwrap().disambiguated_name();
            let display_name2 = room.get_member(&user_id2).unwrap().disambiguated_name();

            assert_eq!("changed", display_name1);
            assert_eq!("example", display_name2);
        }

        // Second member *also* changes his display name to "changed". Again, both display name are
        // disambiguated.
        client
            .receive_sync_response(&mut member2_name_change_sync_response)
            .await
            .unwrap();

        {
            let room = client.get_joined_room(&room_id).await.unwrap();
            let room = room.read().await;

            let display_name1 = room.get_member(&user_id1).unwrap().disambiguated_name();
            let display_name2 = room.get_member(&user_id2).unwrap().disambiguated_name();

            assert_eq!(format!("changed ({})", user_id1), display_name1);
            assert_eq!(format!("changed ({})", user_id2), display_name2);
        }
    }

    #[async_test]
    async fn room_events() {
        let client = get_client().await;
        let room_id = get_room_id();
        let user_id = UserId::try_from("@example:localhost").unwrap();

        let mut response = EventBuilder::default()
            .add_state_event(EventsJson::Member)
            .add_state_event(EventsJson::PowerLevels)
            .build_sync_response();

        client.receive_sync_response(&mut response).await.unwrap();

        let room = client.get_joined_room(&room_id).await.unwrap();
        let room = room.read().await;

        assert_eq!(room.joined_members.len(), 1);
        assert!(room.power_levels.is_some());
        assert_eq!(
            room.power_levels.as_ref().unwrap().kick,
            crate::js_int::int!(50)
        );
        let admin = room.joined_members.get(&user_id).unwrap();
        assert_eq!(admin.power_level.unwrap(), crate::js_int::int!(100));
    }

    #[async_test]
    async fn calculate_aliases() {
        let client = get_client().await;

        let room_id = get_room_id();

        let mut response = EventBuilder::default()
            .add_state_event(EventsJson::Aliases)
            .build_sync_response();

        client.receive_sync_response(&mut response).await.unwrap();

        let room = client.get_joined_room(&room_id).await.unwrap();
        let room = room.read().await;

        assert_eq!("tutorial", room.display_name());
    }

    #[async_test]
    async fn calculate_alias() {
        let client = get_client().await;

        let room_id = get_room_id();

        let mut response = EventBuilder::default()
            .add_state_event(EventsJson::Alias)
            .build_sync_response();

        client.receive_sync_response(&mut response).await.unwrap();

        let room = client.get_joined_room(&room_id).await.unwrap();
        let room = room.read().await;

        assert_eq!("tutorial", room.display_name());
    }

    #[async_test]
    async fn calculate_name() {
        let client = get_client().await;

        let room_id = get_room_id();

        let mut response = EventBuilder::default()
            .add_state_event(EventsJson::Name)
            .build_sync_response();

        client.receive_sync_response(&mut response).await.unwrap();

        let room = client.get_joined_room(&room_id).await.unwrap();
        let room = room.read().await;

        assert_eq!("room name", room.display_name());
    }

    #[async_test]
    async fn calculate_room_names_from_summary() {
        let mut response = sync_response(SyncResponseFile::DefaultWithSummary);

        let session = Session {
            access_token: "1234".to_owned(),
            user_id: UserId::try_from("@example:localhost").unwrap(),
            device_id: "DEVICEID".into(),
        };
        let client = BaseClient::new().unwrap();
        client.restore_login(session).await.unwrap();
        client.receive_sync_response(&mut response).await.unwrap();

        let mut room_names = vec![];
        for room in client.joined_rooms().read().await.values() {
            room_names.push(room.read().await.display_name())
        }

        assert_eq!(vec!["example2"], room_names);
    }

    #[cfg(feature = "messages")]
    #[async_test]
    async fn message_queue_redaction_event() {
        let room_id = get_room_id();

        let mut response = sync_response(SyncResponseFile::DefaultWithSummary);

        let session = Session {
            access_token: "1234".to_owned(),
            user_id: UserId::try_from("@example:localhost").unwrap(),
            device_id: "DEVICEID".into(),
        };
        let client = BaseClient::new().unwrap();
        client.restore_login(session).await.unwrap();
        client.receive_sync_response(&mut response).await.unwrap();

        let json = serde_json::json!({
            "content": {
                "reason": "😀"
            },
            "event_id": "$151957878228ssqrJ:localhost",
            "origin_server_ts": 151957878,
            "sender": "@example:localhost",
            "type": "m.room.redaction",
            "redacts": "$152037280074GZeOm:localhost"
        });
        let mut event: Raw<AnySyncRoomEvent> = serde_json::from_value(json).unwrap();
        client
            .receive_joined_timeline_event(&room_id, &mut event)
            .await
            .unwrap();

        for room in client.joined_rooms().read().await.values() {
            let queue = &room.read().await.messages;
            if let crate::events::AnyPossiblyRedactedSyncMessageEvent::Redacted(
                crate::events::AnyRedactedSyncMessageEvent::RoomMessage(event),
            ) = &queue.msgs[0]
            {
                // this is the id from the message event in the sync response
                assert_eq!(
                    event.event_id,
                    EventId::try_from("$152037280074GZeOm:localhost").unwrap()
                )
            } else {
                panic!("message event in message queue should be redacted")
            }
        }
    }

    #[async_test]
    #[cfg(not(target_arch = "wasm32"))]
    async fn encryption_info_test() {
        let room_id = get_room_id();

        let mut response = sync_response(SyncResponseFile::DefaultWithSummary);
        let user_id = UserId::try_from("@example:localhost").unwrap();

        let session = Session {
            access_token: "1234".to_owned(),
            user_id: user_id.clone(),
            device_id: "DEVICEID".into(),
        };
        let client = BaseClient::new().unwrap();
        client.restore_login(session).await.unwrap();
        client.receive_sync_response(&mut response).await.unwrap();

        let mut content = EncryptionEventContent::new(Algorithm::MegolmV1AesSha2);
        content.rotation_period_ms = Some(100_000u32.into());
        content.rotation_period_msgs = Some(100u32.into());

        let event = SyncStateEvent {
            event_id: EventId::try_from("$h29iv0s8:example.com").unwrap(),
            origin_server_ts: SystemTime::now(),
            sender: user_id,
            state_key: "".into(),
            unsigned: Unsigned::default(),
            content,
            prev_content: None,
        };

        let room = client.get_joined_room(&room_id).await.unwrap();

        assert!(!room.read().await.is_encrypted());
        room.write().await.handle_encryption_event(&event);
        assert!(room.read().await.is_encrypted());

        let room_lock = room.read().await;
        let encryption_info = room_lock.encryption_info().unwrap();

        assert_eq!(encryption_info.algorithm(), &Algorithm::MegolmV1AesSha2);
        assert_eq!(encryption_info.rotation_period(), 100_000);
        assert_eq!(encryption_info.rotation_period_messages(), 100);
    }
}
