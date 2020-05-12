use std::convert::TryFrom;
use std::panic;

use http::Response;

use matrix_sdk_common::api::r0::sync::sync_events::Response as SyncResponse;
use matrix_sdk_common::events::{
    collections::{
        all::{RoomEvent, StateEvent},
        only::Event,
    },
    presence::PresenceEvent,
    EventJson, TryFromRaw,
};

pub use matrix_sdk_test_macros::async_test;

/// Embedded event files
#[derive(Debug)]
pub enum EventsFile {
    Alias,
    Aliases,
    Create,
    FullyRead,
    HistoryVisibility,
    JoinRules,
    Member,
    MessageEmote,
    MessageNotice,
    MessageText,
    Name,
    PowerLevels,
    Presence,
    RedactedInvalid,
    RedactedState,
    Redacted,
    Redaction,
    RoomAvatar,
    Tag,
    Topic,
    Typing,
}

/// Easily create events to stream into either a Client or a `Room` for testing.
#[derive(Default)]
pub struct EventBuilder {
    /// The events that determine the state of a `Room`.
    room_events: Vec<RoomEvent>,
    /// The presence events that determine the presence state of a `RoomMember`.
    presence_events: Vec<PresenceEvent>,
    /// The state events that determine the state of a `Room`.
    state_events: Vec<StateEvent>,
    /// The ephemeral room events that determine the state of a `Room`.
    ephemeral: Vec<Event>,
    /// The account data events that determine the state of a `Room`.
    account_data: Vec<Event>,
}

impl EventBuilder {
    /// Add an event to the room events `Vec`.
    pub fn add_ephemeral<Ev: TryFromRaw>(
        mut self,
        file: EventsFile,
        variant: fn(Ev) -> Event,
    ) -> Self {
        let val: &str = match file {
            EventsFile::Typing => include_str!("../../test_data/events/typing.json"),
            _ => panic!("unknown ephemeral event file {:?}", file),
        };

        let event = serde_json::from_str::<EventJson<Ev>>(&val)
            .unwrap()
            .deserialize()
            .unwrap();
        self.ephemeral.push(variant(event));
        self
    }

    /// Add an event to the room events `Vec`.
    #[allow(clippy::match_single_binding, unused)]
    pub fn add_account<Ev: TryFromRaw>(
        mut self,
        file: EventsFile,
        variant: fn(Ev) -> Event,
    ) -> Self {
        let val: &str = match file {
            _ => panic!("unknown account event file {:?}", file),
        };

        let event = serde_json::from_str::<EventJson<Ev>>(&val)
            .unwrap()
            .deserialize()
            .unwrap();
        self.account_data.push(variant(event));
        self
    }

    /// Add an event to the room events `Vec`.
    pub fn add_room_event<Ev: TryFromRaw>(
        mut self,
        file: EventsFile,
        variant: fn(Ev) -> RoomEvent,
    ) -> Self {
        let val = match file {
            EventsFile::Member => include_str!("../../test_data/events/member.json"),
            EventsFile::PowerLevels => include_str!("../../test_data/events/power_levels.json"),
            _ => panic!("unknown room event file {:?}", file),
        };

        let event = serde_json::from_str::<EventJson<Ev>>(&val)
            .unwrap()
            .deserialize()
            .unwrap();
        self.room_events.push(variant(event));
        self
    }

    /// Add a state event to the state events `Vec`.
    pub fn add_state_event<Ev: TryFromRaw>(
        mut self,
        file: EventsFile,
        variant: fn(Ev) -> StateEvent,
    ) -> Self {
        let val = match file {
            EventsFile::Alias => include_str!("../../test_data/events/alias.json"),
            EventsFile::Aliases => include_str!("../../test_data/events/aliases.json"),
            EventsFile::Name => include_str!("../../test_data/events/name.json"),
            _ => panic!("unknown state event file {:?}", file),
        };

        let event = serde_json::from_str::<EventJson<Ev>>(&val)
            .unwrap()
            .deserialize()
            .unwrap();
        self.state_events.push(variant(event));
        self
    }

    /// Add an presence event to the presence events `Vec`.
    pub fn add_presence_event(mut self, file: EventsFile) -> Self {
        let val = match file {
            EventsFile::Presence => include_str!("../../test_data/events/presence.json"),
            _ => panic!("unknown presence event file {:?}", file),
        };

        let event = serde_json::from_str::<EventJson<PresenceEvent>>(&val)
            .unwrap()
            .deserialize()
            .unwrap();
        self.presence_events.push(event);
        self
    }

    /// Consumes `ResponseBuilder and returns SyncResponse.
    pub fn build_sync_response(self) -> SyncResponse {
        let body = serde_json::json! {
            {
                "device_one_time_keys_count": {},
                "next_batch": "s526_47314_0_7_1_1_1_11444_1",
                "device_lists": {
                    "changed": [],
                    "left": []
                },
                "rooms": {
                    "invite": {},
                    "join": {
                        "!SVkFJHzfwvuaIEawgC:localhost": {
                            "summary": {},
                            "account_data": {
                                "events": self.account_data
                            },
                            "ephemeral": {
                                "events": self.ephemeral
                            },
                            "state": {
                                "events": self.state_events
                            },
                            "timeline": {
                                "events": self.room_events,
                                "limited": true,
                                "prev_batch": "t392-516_47314_0_7_1_1_1_11444_1"
                            },
                            "unread_notifications": {
                                "highlight_count": 0,
                                "notification_count": 11
                            }
                        }
                    },
                    "leave": {}
                },
                "to_device": {
                    "events": []
                },
                "presence": {
                    "events": []
                }
            }
        };
        let response = Response::builder()
            .body(serde_json::to_vec(&body).unwrap())
            .unwrap();
        SyncResponse::try_from(response).unwrap()
    }
}

/// Embedded sync reponse files
pub enum SyncResponseFile {
    Default,
    DefaultWithSummary,
    Invite,
    Leave,
}

/// Get specific API responses for testing
pub fn sync_response(kind: SyncResponseFile) -> SyncResponse {
    let data = match kind {
        SyncResponseFile::Default => include_bytes!("../../test_data/sync.json").to_vec(),
        SyncResponseFile::DefaultWithSummary => {
            include_bytes!("../../test_data/sync_with_summary.json").to_vec()
        }
        SyncResponseFile::Invite => include_bytes!("../../test_data/invite_sync.json").to_vec(),
        SyncResponseFile::Leave => include_bytes!("../../test_data/leave_sync.json").to_vec(),
    };

    let response = Response::builder().body(data.to_vec()).unwrap();
    SyncResponse::try_from(response).unwrap()
}
