mod members;
mod normal;

use matrix_sdk_common::{
    events::room::{
        create::CreateEventContent, guest_access::GuestAccess,
        history_visibility::HistoryVisibility, join_rules::JoinRule,
    },
    identifiers::{MxcUri, UserId},
};
pub use normal::{Room, RoomInfo, RoomType};

pub use members::RoomMember;

use serde::{Deserialize, Serialize};
use std::cmp::max;

use matrix_sdk_common::{
    events::{
        room::{encryption::EncryptionEventContent, tombstone::TombstoneEventContent},
        AnyStateEventContent,
    },
    identifiers::RoomAliasId,
};

/// A base room info struct that is the backbone of normal as well as stripped
/// rooms. Holds all the state events that are important to present a room to
/// users.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BaseRoomInfo {
    /// The avatar URL of this room.
    pub avatar_url: Option<MxcUri>,
    /// The canonical alias of this room.
    pub canonical_alias: Option<RoomAliasId>,
    /// The `m.room.create` event content of this room.
    pub create: Option<CreateEventContent>,
    /// The user id this room is sharing the direct message with, if the room is
    /// a direct message.
    pub dm_target: Option<UserId>,
    /// The `m.room.encryption` event content that enabled E2EE in this room.
    pub encryption: Option<EncryptionEventContent>,
    /// The guest access policy of this room.
    pub guest_access: GuestAccess,
    /// The history visibility policy of this room.
    pub history_visibility: HistoryVisibility,
    /// The join rule policy of this room.
    pub join_rule: JoinRule,
    /// The maximal power level that can be found in this room.
    pub max_power_level: i64,
    /// The `m.room.name` of this room.
    pub name: Option<String>,
    /// The `m.room.tombstone` event content of this room.
    pub tombstone: Option<TombstoneEventContent>,
    /// The topic of this room.
    pub topic: Option<String>,
}

impl BaseRoomInfo {
    /// Create a new, empty base room info.
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn calculate_room_name(
        &self,
        joined_member_count: u64,
        invited_member_count: u64,
        heroes: Vec<RoomMember>,
    ) -> String {
        let heroes_count = heroes.len() as u64;
        let invited_joined = (invited_member_count + joined_member_count).saturating_sub(1);

        if heroes_count >= invited_joined {
            let mut names = heroes
                .iter()
                .take(3)
                .map(|mem| mem.name())
                .collect::<Vec<&str>>();
            // stabilize ordering
            names.sort_unstable();
            names.join(", ")
        } else if heroes_count < invited_joined && invited_joined > 1 {
            let mut names = heroes
                .iter()
                .take(3)
                .map(|mem| mem.name())
                .collect::<Vec<&str>>();
            names.sort_unstable();

            // TODO: What length does the spec want us to use here and in
            // the `else`?
            format!(
                "{}, and {} others",
                names.join(", "),
                (joined_member_count + invited_member_count)
            )
        } else {
            "Empty room".to_string()
        }
    }

    /// Handle a state event for this room and update our info accordingly.
    ///
    /// Returns true if the event modified the info, false otherwise.
    pub fn handle_state_event(&mut self, content: &AnyStateEventContent) -> bool {
        match content {
            AnyStateEventContent::RoomEncryption(encryption) => {
                self.encryption = Some(encryption.clone());
                true
            }
            AnyStateEventContent::RoomAvatar(a) => {
                self.avatar_url = a.url.clone();
                true
            }
            AnyStateEventContent::RoomName(n) => {
                self.name = n.name().map(|n| n.to_string());
                true
            }
            AnyStateEventContent::RoomCreate(c) => {
                if self.create.is_none() {
                    self.create = Some(c.clone());
                    true
                } else {
                    false
                }
            }
            AnyStateEventContent::RoomHistoryVisibility(h) => {
                self.history_visibility = h.history_visibility.clone();
                true
            }
            AnyStateEventContent::RoomGuestAccess(g) => {
                self.guest_access = g.guest_access.clone();
                true
            }
            AnyStateEventContent::RoomJoinRules(c) => {
                self.join_rule = c.join_rule.clone();
                true
            }
            AnyStateEventContent::RoomCanonicalAlias(a) => {
                self.canonical_alias = a.alias.clone();
                true
            }
            AnyStateEventContent::RoomTopic(t) => {
                self.topic = Some(t.topic.clone());
                true
            }
            AnyStateEventContent::RoomTombstone(t) => {
                self.tombstone = Some(t.clone());
                true
            }
            AnyStateEventContent::RoomPowerLevels(p) => {
                let max_power_level = p
                    .users
                    .values()
                    .fold(self.max_power_level, |acc, p| max(acc, (*p).into()));
                self.max_power_level = max_power_level;
                true
            }
            _ => false,
        }
    }
}

impl Default for BaseRoomInfo {
    fn default() -> Self {
        Self {
            avatar_url: None,
            canonical_alias: None,
            create: None,
            dm_target: None,
            encryption: None,
            guest_access: GuestAccess::CanJoin,
            history_visibility: HistoryVisibility::WorldReadable,
            join_rule: JoinRule::Public,
            max_power_level: 100,
            name: None,
            tombstone: None,
            topic: None,
        }
    }
}
