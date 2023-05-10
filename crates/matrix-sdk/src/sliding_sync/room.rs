use std::{fmt::Debug, ops::Not};

use eyeball_im::Vector;
use matrix_sdk_base::deserialized_responses::SyncTimelineEvent;
use ruma::{
    api::client::sync::sync_events::{v4, UnreadNotificationsCount},
    events::AnySyncStateEvent,
    serde::Raw,
    OwnedRoomId,
};
use serde::{Deserialize, Serialize};
use tracing::{error, instrument};

use crate::{
    room::timeline::{EventTimelineItem, Timeline, TimelineBuilder},
    Client,
};

/// A Sliding Sync Room.
///
/// It contains some information about a specific room, along with a queue of
/// events for the timeline.
#[derive(Debug, Clone)]
pub struct SlidingSyncRoom {
    /// The client, used to fetch [`Room`][crate::room::Room].
    client: Client,

    /// The room ID.
    room_id: OwnedRoomId,

    /// The room representation as a `SlidingSync`'s response from the server.
    ///
    /// We update this response when an update is needed.
    inner: v4::SlidingSyncRoom,

    /// Internal state of `Self`.
    state: SlidingSyncRoomState,

    /// A queue of received events, used to build a
    /// [`Timeline`][crate::Timeline].
    timeline_queue: Vector<SyncTimelineEvent>,
}

/// The state of a [`SlidingSyncRoom`].
#[derive(Debug, Default, Clone)]
pub enum SlidingSyncRoomState {
    /// The room is not loaded, i.e. not updates have been received yet.
    #[default]
    NotLoaded,

    /// The room has been preloaded, i.e. its values come from the cache, but no
    /// updates have been received yet.
    Preloaded,

    /// The room has received updates.
    Loaded,
}

impl SlidingSyncRoom {
    pub(super) fn new(
        client: Client,
        room_id: OwnedRoomId,
        inner: v4::SlidingSyncRoom,
        timeline: Vec<SyncTimelineEvent>,
    ) -> Self {
        Self {
            client,
            room_id,
            inner,
            state: SlidingSyncRoomState::NotLoaded,
            timeline_queue: timeline.into(),
        }
    }

    /// RoomId of this SlidingSyncRoom
    pub fn room_id(&self) -> &OwnedRoomId {
        &self.room_id
    }

    /// `Timeline` of this room
    pub async fn timeline(&self) -> Option<Timeline> {
        Some(self.timeline_builder()?.track_read_marker_and_receipts().build().await)
    }

    fn timeline_builder(&self) -> Option<TimelineBuilder> {
        if let Some(room) = self.client.get_room(&self.room_id) {
            Some(
                Timeline::builder(&room)
                    .events(self.inner.prev_batch.clone(), self.timeline_queue.clone()),
            )
        } else if let Some(invited_room) = self.client.get_invited_room(&self.room_id) {
            Some(Timeline::builder(&invited_room).events(None, Vector::new()))
        } else {
            error!(
                room_id = ?self.room_id,
                "Room not found in client. Can't provide a timeline for it"
            );

            None
        }
    }

    /// The latest timeline item of this room.
    ///
    /// Use `Timeline::latest_event` instead if you already have a timeline for
    /// this `SlidingSyncRoom`.
    #[instrument(skip_all)]
    pub async fn latest_event(&self) -> Option<EventTimelineItem> {
        self.timeline_builder()?.build().await.latest_event().await
    }

    /// This rooms name as calculated by the server, if any
    pub fn name(&self) -> Option<&str> {
        self.inner.name.as_deref()
    }

    /// Is this a direct message?
    pub fn is_dm(&self) -> Option<bool> {
        self.inner.is_dm
    }

    /// Was this an initial response.
    pub fn is_initial_response(&self) -> Option<bool> {
        self.inner.initial
    }

    /// Is there any unread notifications?
    pub fn has_unread_notifications(&self) -> bool {
        self.inner.unread_notifications.is_empty().not()
    }

    /// Get unread notifications.
    pub fn unread_notifications(&self) -> &UnreadNotificationsCount {
        &self.inner.unread_notifications
    }

    /// Get the required state.
    pub fn required_state(&self) -> &Vec<Raw<AnySyncStateEvent>> {
        &self.inner.required_state
    }

    pub(super) fn update(
        &mut self,
        room_data: v4::SlidingSyncRoom,
        timeline_updates: Vec<SyncTimelineEvent>,
    ) {
        let v4::SlidingSyncRoom {
            name,
            initial,
            limited,
            is_dm,
            invite_state,
            unread_notifications,
            required_state,
            prev_batch,
            ..
        } = room_data;

        self.inner.unread_notifications = unread_notifications;

        // The server might not send some parts of the response, because they were sent
        // before and the server wants to save bandwidth. So let's update the values
        // only when they exist.

        if name.is_some() {
            self.inner.name = name;
        }

        if initial.is_some() {
            self.inner.initial = initial;
        }

        if is_dm.is_some() {
            self.inner.is_dm = is_dm;
        }

        if invite_state.is_some() {
            self.inner.invite_state = invite_state;
        }

        if !required_state.is_empty() {
            self.inner.required_state = required_state;
        }

        if prev_batch.is_some() {
            self.inner.prev_batch = prev_batch;
        }

        // There is timeline updates.
        if !timeline_updates.is_empty() {
            if let SlidingSyncRoomState::Preloaded = self.state {
                // If the room has been read from the cache, we overwrite the timeline queue
                // with the timeline updates.

                self.timeline_queue.clear();
                self.timeline_queue.extend(timeline_updates);
            } else if limited {
                // The server alerted us that we missed items in between.

                self.timeline_queue.clear();
                self.timeline_queue.extend(timeline_updates);
            } else {
                // It's the hot path. We have new updates that must be added to the existing
                // timeline queue.

                self.timeline_queue.extend(timeline_updates);
            }
        } else if limited {
            // The timeline updates are empty. But `limited` is set to true. It's a way to
            // alert that we are stale. In this case, we should just clear the
            // existing timeline.

            self.timeline_queue.clear();
        }

        self.state = SlidingSyncRoomState::Loaded;
    }

    pub(super) fn from_frozen(frozen_room: FrozenSlidingSyncRoom, client: Client) -> Self {
        let FrozenSlidingSyncRoom { room_id, inner, timeline_queue } = frozen_room;

        Self { client, room_id, inner, state: SlidingSyncRoomState::Preloaded, timeline_queue }
    }
}

/// A “frozen” [`SlidingSyncRoom`], i.e. that can be written into, or read from
/// a store.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct FrozenSlidingSyncRoom {
    pub(super) room_id: OwnedRoomId,
    pub(super) inner: v4::SlidingSyncRoom,
    #[serde(rename = "timeline")]
    pub(super) timeline_queue: Vector<SyncTimelineEvent>,
}

impl From<&SlidingSyncRoom> for FrozenSlidingSyncRoom {
    fn from(value: &SlidingSyncRoom) -> Self {
        let timeline = &value.timeline_queue;
        let timeline_length = timeline.len();

        let mut inner = value.inner.clone();

        // To not overflow the database, we only freeze the newest 10 items. On doing
        // so, we must drop the `prev_batch` key however, as we'd otherwise
        // create a gap between what we have loaded and where the
        // prev_batch-key will start loading when paginating backwards.
        let timeline = if timeline_length > 10 {
            let pos = timeline_length - 10;
            inner.prev_batch = None;
            timeline.iter().skip(pos).cloned().collect()
        } else {
            timeline.clone()
        };

        Self { room_id: value.room_id.clone(), inner, timeline_queue: timeline }
    }
}

#[cfg(test)]
mod tests {
    use imbl::vector;
    use matrix_sdk_base::deserialized_responses::TimelineEvent;
    use ruma::{events::room::message::RoomMessageEventContent, room_id};
    use serde_json::json;

    use super::*;

    #[test]
    fn test_frozen_sliding_sync_room_serialization() {
        let frozen_sliding_sync_room = FrozenSlidingSyncRoom {
            room_id: room_id!("!29fhd83h92h0:example.com").to_owned(),
            inner: v4::SlidingSyncRoom::default(),
            timeline_queue: vector![TimelineEvent::new(
                Raw::new(&json! ({
                    "content": RoomMessageEventContent::text_plain("let it gooo!"),
                    "type": "m.room.message",
                    "event_id": "$xxxxx:example.org",
                    "room_id": "!someroom:example.com",
                    "origin_server_ts": 2189,
                    "sender": "@bob:example.com",
                }))
                .unwrap()
                .cast(),
            )
            .into()],
        };

        assert_eq!(
            serde_json::to_string(&frozen_sliding_sync_room).unwrap(),
            r#"{"room_id":"!29fhd83h92h0:example.com","inner":{},"timeline":[{"event":{"content":{"body":"let it gooo!","msgtype":"m.text"},"event_id":"$xxxxx:example.org","origin_server_ts":2189,"room_id":"!someroom:example.com","sender":"@bob:example.com","type":"m.room.message"},"encryption_info":null}]}"#,
        );
    }
}
