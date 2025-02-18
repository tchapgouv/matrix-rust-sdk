//! Types for the [`im.vector.room.access_rules`] event.
//!
//!
//! 

use ruma::events::{macros::EventContent, EmptyStateKey};
use serde::{
    Deserialize, Serialize,
};

/// The rule used for users wishing to join this room.
///
/// This type can hold an arbitrary string. To check for values that are not available as a
/// documented variant here, use its string representation, obtained through `.as_str()`.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
//#[cfg_attr(not(ruma_unstable_exhaustive_types), non_exhaustive)]
#[serde(rename_all = "snake_case")]
pub enum AccessRule {
    /// A user who wishes to join the room must first receive an invite to the room from someone
    /// already inside of the room.
    Restricted,

    /// Users can join the room if they are invited, or they can request an invite to the room.
    ///
    /// They can be allowed (invited) or denied (kicked/banned) access.
    Unrestricted,
}

impl AccessRule { }

/// The content of an `m.room.join_rules` event.
///
/// Describes how users are allowed to join the room.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Clone, Debug, Serialize, Deserialize, EventContent)]
//#[cfg_attr(not(ruma_unstable_exhaustive_types), non_exhaustive)]
#[ruma_event(type = "im.vector.room.access_rules", kind = State, state_key_type = EmptyStateKey)]
pub struct RoomAccessRulesEventContent {
    /// The type of rules used for users wishing to join this room.
    #[serde(rename = "rule")] // The key name awaited by Synapse.
    pub access_rule: AccessRule,
}

impl RoomAccessRulesEventContent {
    /// Creates a new `RoomAccessRulesEventContent` with the given rule.
    pub fn new(access_rule: AccessRule) -> Self {
        Self { access_rule }
    }
}

// impl<'de> Deserialize<'de> for RoomAccessRulesEventContent {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         let access_rule = AccessRule::deserialize(deserializer)?;
//         Ok(RoomAccessRulesEventContent { access_rule })
//     }
// }

// struct RoomAccessRulesEventWrapper(RoomAccessRulesEvent);

// impl RoomAccessRulesEvent for RoomAccessRulesEventWrapper {
//     /// Obtain the join rule, regardless of whether this event is redacted.
//     pub fn access_rule(&self) -> &AccessRule {
//         match self {
//             Self::Original(ev) => &ev.content.access_rule,
//             Self::Redacted(ev) => &ev.content.access_rule,
//         }
//     }
// }

// impl SyncRoomAccessRulesEvent {
//     /// Obtain the join rule, regardless of whether this event is redacted.
//     pub fn access_rule(&self) -> &AccessRule {
//         match self {
//             Self::Original(ev) => &ev.content.access_rule,
//             Self::Redacted(ev) => &ev.content.access_rule,
//         }
//     }
// }



#[cfg(test)]
mod tests {
    use assert_matches2::assert_matches;

    use super::{
        AccessRule, RoomAccessRulesEventContent,
    };

    #[test]
    fn unrestricted_room() {
        let a = RoomAccessRulesEventContent { access_rule: AccessRule::Unrestricted };
        println!("RoomAccessRulesEventContent: {}", serde_json::to_string(&a).unwrap_or("???".to_owned()));
        let a = AccessRule::Unrestricted;
        println!("AccessRule: {}", serde_json::to_string(&a).unwrap_or("???".to_owned()));
        let json = r#"{"rule":"unrestricted"}"#;
        let event: RoomAccessRulesEventContent = serde_json::from_str(json).unwrap();
        assert_matches!(event, RoomAccessRulesEventContent { access_rule: AccessRule::Unrestricted });
    }

    #[test]
    fn restricted_room() {
        let json = r#"{"rule":"restricted"}"#;
        let access_rules: RoomAccessRulesEventContent = serde_json::from_str(json).unwrap();
        assert_matches!(
            access_rules,
            RoomAccessRulesEventContent { access_rule: AccessRule::Restricted }
        );
    }

    // #[test]
    // fn join_rule_to_space_room_join_rule() {
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






/*
// use std::cmp::Ordering;

use ruma::{events::{macros::EventContent, EmptyStateKey, RedactContent, RedactedStateEventContent, SyncStateEvent}, RoomVersionId};
//use ruma::serde::StringEnum;
use serde::{Deserialize, Serialize};
use crate::deserialized_responses::SyncOrStrippedState;


// Question: define `RoomAccessRules` as a Tchap feature?
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Clone, Debug, Deserialize, Serialize)]
//#[cfg_attr(not(ruma_unstable_exhaustive_types), non_exhaustive)]
/// RoomAccessRules defines if the room will be accessible to external user (in Tchap meaning).
pub enum RoomAccessRules {
    /// Indicates that the room will not be open to external user (in Tchap meaning).
    #[serde(rename = "restricted")]
    Restricted,
    /// Indicates that the room will be open to external user (in Tchap meaning).
    #[serde(rename = "unrestricted")]
    Unrestricted,
}


/// RoomAccessRules custom StateEvent:
// #[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Clone, Debug, Serialize, EventContent)]
#[cfg_attr(not(ruma_unstable_exhaustive_types), non_exhaustive)]
#[ruma_event(type = "im.vector.room.access_rules", kind = State, state_key_type = EmptyStateKey)]
pub struct RoomAccessRulesEventContent {
    /// The rule value.
    #[ruma_event(skip_redaction)]
    #[serde(flatten)]
   pub rule: RoomAccessRules,
}

impl RoomAccessRulesEventContent {
    /// Creates a new `RoomAccessRulesEventContent` with the given rule.
    pub fn new(rule: RoomAccessRules) -> Self {
        Self { rule }
    }
}

/// Redacted form of [`RoomAccessRulesEventContent`].
pub type RedactedRoomAccessRulesEventContent = RoomAccessRulesEventContent;

impl RedactedStateEventContent for RedactedRoomAccessRulesEventContent {
    type StateKey = EmptyStateKey;
}

impl RedactContent for RedactedRoomAccessRulesEventContent {
    type Redacted = RedactedRoomAccessRulesEventContent;

    fn redact(self, _: &RoomVersionId) -> Self::Redacted {
        self
    }
}

impl SyncOrStrippedState<RoomAccessRulesEventContent> {
    /// The power levels of the event.
    pub fn access_rules(&self) -> Option<RoomAccessRules> {
        match self {
            Self::Sync(e) => {
                match e {
                    SyncStateEvent::Original(ev) => Some(ev.content.rule.clone()),
                    SyncStateEvent::Redacted(ev) => Some(ev.content.rule.clone()),
                }
            },
            Self::Stripped(_) => {
                None
            },
        }
    }
}
*/