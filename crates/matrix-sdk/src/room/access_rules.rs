//! Types for the [`im.vector.room.access_rules`] event.

use ruma::events::{macros::EventContent, EmptyStateKey};
use serde::{Deserialize, Serialize};

/// The rule used for Tchap external users wishing to join this room.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
//#[cfg_attr(not(ruma_unstable_exhaustive_types), non_exhaustive)]
#[serde(rename_all = "snake_case")]
pub enum AccessRule {
    /// For Direct message, a room between only 2 users.
    Direct,

    /// A external user who wishes to join the room must first receive an invite
    /// to the room from someone already inside of the room.
    Restricted,

    /// External users can join the room if they are invited.
    ///
    /// They can be allowed (invited) or denied (kicked/banned) access.
    Unrestricted,
}

impl AccessRule {}

/// The content of an `im.vector.room.access_rules` event.
///
/// Describes how external users are allowed to join the room.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Clone, Debug, Serialize, Deserialize, EventContent)]
//#[cfg_attr(not(ruma_unstable_exhaustive_types), non_exhaustive)]
#[ruma_event(type = "im.vector.room.access_rules", kind = State, state_key_type = EmptyStateKey)]
pub struct RoomAccessRulesEventContent {
    /// The type of rules used for external users wishing to join this room.
    #[serde(rename = "rule")] // The key name awaited by Synapse. Mandatory!
    pub access_rule: AccessRule,
}

impl RoomAccessRulesEventContent {
    /// Creates a new `RoomAccessRulesEventContent` with the given rule.
    pub fn new(access_rule: AccessRule) -> Self {
        Self { access_rule }
    }
}

#[cfg(test)]
mod tests {
    use assert_matches2::assert_matches;

    use super::{AccessRule, RoomAccessRulesEventContent};

    #[test]
    fn unrestricted_access_rule_room() {
        // let a = RoomAccessRulesEventContent { access_rule: AccessRule::Unrestricted
        // }; println!("RoomAccessRulesEventContent: {}",
        // serde_json::to_string(&a).unwrap_or("???".to_owned()));
        // let a = AccessRule::Unrestricted;
        // println!("AccessRule: {}",
        // serde_json::to_string(&a).unwrap_or("???".to_owned()));
        let json = r#"{"rule":"unrestricted"}"#;
        let event: RoomAccessRulesEventContent = serde_json::from_str(json).unwrap();
        assert_matches!(
            event,
            RoomAccessRulesEventContent { access_rule: AccessRule::Unrestricted }
        );
    }

    #[test]
    fn restricted_access_rule_room() {
        let json = r#"{"rule":"restricted"}"#;
        let access_rules: RoomAccessRulesEventContent = serde_json::from_str(json).unwrap();
        assert_matches!(
            access_rules,
            RoomAccessRulesEventContent { access_rule: AccessRule::Restricted }
        );
    }

    #[test]
    fn direct_access_rule_room() {
        let json = r#"{"rule":"direct"}"#;
        let access_rules: RoomAccessRulesEventContent = serde_json::from_str(json).unwrap();
        assert_matches!(
            access_rules,
            RoomAccessRulesEventContent { access_rule: AccessRule::Direct }
        );
    }

    // Copied from Join Rule
    //
    // #[test]
    // fn access_rule_to_space_room_access_rule() {
    //     assert_eq!(SpaceRoomJoinRule::Invite, JoinRule::Invite.into());
    //     assert_eq!(SpaceRoomJoinRule::Knock, JoinRule::Knock.into());
    //     assert_eq!(
    //         SpaceRoomJoinRule::KnockRestricted,
    //         JoinRule::KnockRestricted(Restricted::default()).into()
    //     );
    //     assert_eq!(SpaceRoomJoinRule::Public, JoinRule::Public.into());
    //     assert_eq!(SpaceRoomJoinRule::Private, JoinRule::Private.into());
    //     assert_eq!(
    //         SpaceRoomJoinRule::Restricted,
    //         JoinRule::Restricted(Restricted::default()).into()
    //     );
    // }
}
