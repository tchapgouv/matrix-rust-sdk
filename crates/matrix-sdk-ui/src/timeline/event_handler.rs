// Copyright 2022 The Matrix.org Foundation C.I.C.
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

use std::sync::Arc;

use eyeball_im::{ObservableVector, ObservableVectorEntry};
use indexmap::{map::Entry, IndexMap};
use matrix_sdk::deserialized_responses::EncryptionInfo;
use ruma::{
    events::{
        reaction::ReactionEventContent,
        receipt::Receipt,
        relation::Replacement,
        room::{
            encrypted::RoomEncryptedEventContent,
            member::RoomMemberEventContent,
            message::{
                self, sanitize::RemoveReplyFallback, RoomMessageEventContent,
                RoomMessageEventContentWithoutRelation,
            },
            redaction::{
                OriginalSyncRoomRedactionEvent, RoomRedactionEventContent, SyncRoomRedactionEvent,
            },
        },
        AnyMessageLikeEventContent, AnySyncMessageLikeEvent, AnySyncStateEvent,
        AnySyncTimelineEvent, BundledMessageLikeRelations, EventContent, FullStateEventContent,
        MessageLikeEventType, StateEventType, SyncStateEvent,
    },
    serde::Raw,
    EventId, MilliSecondsSinceUnixEpoch, OwnedEventId, OwnedTransactionId, OwnedUserId,
};
use tracing::{debug, error, field::debug, info, instrument, trace, warn};

use super::{
    event_item::{
        AnyOtherFullStateEventContent, BundledReactions, EventItemIdentifier, EventSendState,
        EventTimelineItemKind, LocalEventTimelineItem, Profile, RemoteEventOrigin,
        RemoteEventTimelineItem,
    },
    item::timeline_item,
    read_receipts::maybe_add_implicit_read_receipt,
    util::{find_read_marker, rfind_event_by_id, rfind_event_item, timestamp_to_date},
    EventTimelineItem, InReplyToDetails, Message, OtherState, ReactionGroup, ReactionSenderData,
    Sticker, TimelineDetails, TimelineInnerState, TimelineItem, TimelineItemContent,
    VirtualTimelineItem, DEFAULT_SANITIZER_MODE,
};
use crate::events::SyncTimelineEventWithoutContent;

#[derive(Clone)]
pub(super) enum Flow {
    Local {
        txn_id: OwnedTransactionId,
    },
    Remote {
        event_id: OwnedEventId,
        txn_id: Option<OwnedTransactionId>,
        raw_event: Raw<AnySyncTimelineEvent>,
        position: TimelineItemPosition,
        should_add: bool,
    },
}

#[derive(Clone)]
pub(super) struct TimelineEventContext {
    pub(super) sender: OwnedUserId,
    pub(super) sender_profile: Option<Profile>,
    pub(super) timestamp: MilliSecondsSinceUnixEpoch,
    pub(super) is_own_event: bool,
    pub(super) encryption_info: Option<EncryptionInfo>,
    pub(super) read_receipts: IndexMap<OwnedUserId, Receipt>,
    pub(super) is_highlighted: bool,
    pub(super) flow: Flow,
}

#[derive(Clone, Debug)]
pub(super) enum TimelineEventKind {
    Message {
        content: AnyMessageLikeEventContent,
        relations: BundledMessageLikeRelations<AnySyncMessageLikeEvent>,
    },
    RedactedMessage {
        event_type: MessageLikeEventType,
    },
    Redaction {
        redacts: OwnedEventId,
        content: RoomRedactionEventContent,
    },
    LocalRedaction {
        redacts: OwnedTransactionId,
        content: RoomRedactionEventContent,
    },
    RoomMember {
        user_id: OwnedUserId,
        content: FullStateEventContent<RoomMemberEventContent>,
        sender: OwnedUserId,
    },
    OtherState {
        state_key: String,
        content: AnyOtherFullStateEventContent,
    },
    FailedToParseMessageLike {
        event_type: MessageLikeEventType,
        error: Arc<serde_json::Error>,
    },
    FailedToParseState {
        event_type: StateEventType,
        state_key: String,
        error: Arc<serde_json::Error>,
    },
}

impl TimelineEventKind {
    pub(super) fn failed_to_parse(
        event: SyncTimelineEventWithoutContent,
        error: serde_json::Error,
    ) -> Self {
        let error = Arc::new(error);
        match event {
            SyncTimelineEventWithoutContent::OriginalMessageLike(ev) => {
                Self::FailedToParseMessageLike { event_type: ev.content.event_type, error }
            }
            SyncTimelineEventWithoutContent::RedactedMessageLike(ev) => {
                Self::FailedToParseMessageLike { event_type: ev.content.event_type, error }
            }
            SyncTimelineEventWithoutContent::OriginalState(ev) => Self::FailedToParseState {
                event_type: ev.content.event_type,
                state_key: ev.state_key,
                error,
            },
            SyncTimelineEventWithoutContent::RedactedState(ev) => Self::FailedToParseState {
                event_type: ev.content.event_type,
                state_key: ev.state_key,
                error,
            },
        }
    }
}

impl From<AnySyncTimelineEvent> for TimelineEventKind {
    fn from(event: AnySyncTimelineEvent) -> Self {
        match event {
            AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomRedaction(
                SyncRoomRedactionEvent::Original(OriginalSyncRoomRedactionEvent {
                    redacts,
                    content,
                    ..
                }),
            )) => Self::Redaction { redacts, content },
            AnySyncTimelineEvent::MessageLike(ev) => match ev.original_content() {
                Some(content) => Self::Message { content, relations: ev.relations() },
                None => Self::RedactedMessage { event_type: ev.event_type() },
            },
            AnySyncTimelineEvent::State(ev) => match ev {
                AnySyncStateEvent::RoomMember(ev) => match ev {
                    SyncStateEvent::Original(ev) => Self::RoomMember {
                        user_id: ev.state_key,
                        content: FullStateEventContent::Original {
                            content: ev.content,
                            prev_content: ev.unsigned.prev_content,
                        },
                        sender: ev.sender,
                    },
                    SyncStateEvent::Redacted(ev) => Self::RoomMember {
                        user_id: ev.state_key,
                        content: FullStateEventContent::Redacted(ev.content),
                        sender: ev.sender,
                    },
                },
                ev => Self::OtherState {
                    state_key: ev.state_key().to_owned(),
                    content: AnyOtherFullStateEventContent::with_event_content(ev.content()),
                },
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum TimelineItemPosition {
    Start,
    End {
        /// Whether this event is coming from a local cache.
        from_cache: bool,
    },
    #[cfg(feature = "e2e-encryption")]
    Update(usize),
}

#[derive(Default)]
pub(super) struct HandleEventResult {
    pub(super) item_added: bool,
    #[cfg(feature = "e2e-encryption")]
    pub(super) item_removed: bool,
    pub(super) items_updated: u16,
}

// Bundles together a few things that are needed throughout the different stages
// of handling an event (figuring out whether it should update an existing
// timeline item, transforming that item or creating a new one, updating the
// reactive Vec).
pub(super) struct TimelineEventHandler<'a> {
    state: &'a mut TimelineInnerState,
    ctx: TimelineEventContext,
    track_read_receipts: bool,
    result: HandleEventResult,
}

// This is a macro instead of a method plus free fn so the borrow checker can
// "see through" it, allowing borrows of TimelineEventHandler fields other than
// `timeline_items` and `items_updated` in the update closure.
macro_rules! update_timeline_item {
    ($this:ident, $event_id:expr, $action:expr, $update:expr) => {
        _update_timeline_item(
            &mut $this.state.items,
            &mut $this.result.items_updated,
            $event_id,
            $action,
            $update,
        )
    };
}

impl<'a> TimelineEventHandler<'a> {
    pub(super) fn new(
        state: &'a mut TimelineInnerState,
        ctx: TimelineEventContext,
        track_read_receipts: bool,
    ) -> Self {
        Self { state, ctx, track_read_receipts, result: HandleEventResult::default() }
    }

    /// Handle an event.
    ///
    /// Returns the number of timeline updates that were made.
    #[instrument(skip_all, fields(txn_id, event_id, position))]
    pub(super) fn handle_event(mut self, event_kind: TimelineEventKind) -> HandleEventResult {
        let span = tracing::Span::current();

        let should_add = match &self.ctx.flow {
            Flow::Local { txn_id, .. } => {
                span.record("txn_id", debug(txn_id));

                true
            }

            Flow::Remote { event_id, txn_id, position, should_add, .. } => {
                span.record("event_id", debug(event_id));
                span.record("position", debug(position));

                if let Some(txn_id) = txn_id {
                    span.record("txn_id", debug(txn_id));
                }

                *should_add
            }
        };

        trace!("Handling event");

        match event_kind {
            TimelineEventKind::Message { content, relations } => match content {
                AnyMessageLikeEventContent::Reaction(c) => {
                    self.handle_reaction(c);
                }
                AnyMessageLikeEventContent::RoomMessage(RoomMessageEventContent {
                    relates_to: Some(message::Relation::Replacement(re)),
                    ..
                }) => {
                    self.handle_room_message_edit(re);
                }
                AnyMessageLikeEventContent::RoomMessage(c) => {
                    self.add(
                        should_add,
                        TimelineItemContent::message(c, relations, &self.state.items),
                    );
                }
                AnyMessageLikeEventContent::RoomEncrypted(c) => self.handle_room_encrypted(c),
                AnyMessageLikeEventContent::Sticker(content) => {
                    self.add(should_add, TimelineItemContent::Sticker(Sticker { content }));
                }
                // TODO
                _ => {
                    debug!(
                        "Ignoring message-like event of type `{}`, not supported (yet)",
                        content.event_type()
                    );
                }
            },

            TimelineEventKind::RedactedMessage { event_type } => {
                if event_type != MessageLikeEventType::Reaction {
                    self.add(should_add, TimelineItemContent::RedactedMessage);
                }
            }

            TimelineEventKind::Redaction { redacts, content } => {
                self.handle_redaction(redacts, content);
            }
            TimelineEventKind::LocalRedaction { redacts, content } => {
                self.handle_local_redaction(redacts, content);
            }

            TimelineEventKind::RoomMember { user_id, content, sender } => {
                self.add(should_add, TimelineItemContent::room_member(user_id, content, sender));
            }

            TimelineEventKind::OtherState { state_key, content } => {
                self.add(
                    should_add,
                    TimelineItemContent::OtherState(OtherState { state_key, content }),
                );
            }

            TimelineEventKind::FailedToParseMessageLike { event_type, error } => {
                self.add(
                    should_add,
                    TimelineItemContent::FailedToParseMessageLike { event_type, error },
                );
            }

            TimelineEventKind::FailedToParseState { event_type, state_key, error } => {
                self.add(
                    should_add,
                    TimelineItemContent::FailedToParseState { event_type, state_key, error },
                );
            }
        }

        if !self.result.item_added {
            trace!("No new item added");

            #[cfg(feature = "e2e-encryption")]
            if let Flow::Remote { position: TimelineItemPosition::Update(idx), .. } = self.ctx.flow
            {
                // If add was not called, that means the UTD event is one that
                // wouldn't normally be visible. Remove it.
                trace!("Removing UTD that was successfully retried");
                self.state.items.remove(idx);
                self.result.item_removed = true;
            }

            // TODO: Add event as raw
        }

        self.result
    }

    #[instrument(skip_all, fields(replacement_event_id = ?replacement.event_id))]
    fn handle_room_message_edit(
        &mut self,
        replacement: Replacement<RoomMessageEventContentWithoutRelation>,
    ) {
        update_timeline_item!(self, &replacement.event_id, "edit", |event_item| {
            if self.ctx.sender != event_item.sender() {
                info!(
                    original_sender = ?event_item.sender(), edit_sender = ?self.ctx.sender,
                    "Edit event applies to another user's timeline item, discarding"
                );
                return None;
            }

            let msg = match &event_item.content() {
                TimelineItemContent::Message(msg) => msg,
                TimelineItemContent::RedactedMessage => {
                    info!("Edit event applies to a redacted message, discarding");
                    return None;
                }
                TimelineItemContent::Sticker(_) => {
                    info!("Edit event applies to a sticker, discarding");
                    return None;
                }
                TimelineItemContent::UnableToDecrypt(_) => {
                    info!("Edit event applies to event that couldn't be decrypted, discarding");
                    return None;
                }
                TimelineItemContent::MembershipChange(_)
                | TimelineItemContent::ProfileChange(_)
                | TimelineItemContent::OtherState { .. } => {
                    info!("Edit event applies to a state event, discarding");
                    return None;
                }
                TimelineItemContent::FailedToParseMessageLike { .. }
                | TimelineItemContent::FailedToParseState { .. } => {
                    info!("Edit event applies to event that couldn't be parsed, discarding");
                    return None;
                }
            };

            let mut msgtype = replacement.new_content.msgtype;
            // Edit's content is never supposed to contain the reply fallback.
            msgtype.sanitize(DEFAULT_SANITIZER_MODE, RemoveReplyFallback::No);

            let new_content = TimelineItemContent::Message(Message {
                msgtype,
                in_reply_to: msg.in_reply_to.clone(),
                edited: true,
            });

            let edit_json = match &self.ctx.flow {
                Flow::Local { .. } => None,
                Flow::Remote { raw_event, .. } => Some(raw_event.clone()),
            };

            trace!("Applying edit");
            Some(event_item.with_content(new_content, edit_json))
        });
    }

    // Redacted reaction events are no-ops so don't need to be handled
    #[instrument(skip_all, fields(relates_to_event_id = ?c.relates_to.event_id))]
    fn handle_reaction(&mut self, c: ReactionEventContent) {
        let event_id: &EventId = &c.relates_to.event_id;
        let (reaction_id, old_txn_id) = match &self.ctx.flow {
            Flow::Local { txn_id, .. } => {
                (EventItemIdentifier::TransactionId(txn_id.clone()), None)
            }
            Flow::Remote { event_id, txn_id, .. } => {
                (EventItemIdentifier::EventId(event_id.clone()), txn_id.as_ref())
            }
        };

        if let Some((idx, event_item)) = rfind_event_by_id(&self.state.items, event_id) {
            let Some(remote_event_item) = event_item.as_remote() else {
                error!("inconsistent state: reaction received on a non-remote event item");
                return;
            };

            // Handling of reactions on redacted events is an open question.
            // For now, ignore reactions on redacted events like Element does.
            if let TimelineItemContent::RedactedMessage = event_item.content() {
                debug!("Ignoring reaction on redacted event");
                return;
            } else {
                let mut reactions = remote_event_item.reactions.clone();
                let reaction_group = reactions.entry(c.relates_to.key.clone()).or_default();

                if let Some(txn_id) = old_txn_id {
                    let id = EventItemIdentifier::TransactionId(txn_id.clone());
                    // Remove the local echo from the related event.
                    if reaction_group.0.remove(&id).is_none() {
                        warn!(
                            "Received reaction with transaction ID, but didn't \
                             find matching reaction in the related event's reactions"
                        );
                    }
                }
                reaction_group.0.insert(
                    reaction_id.clone(),
                    ReactionSenderData {
                        sender_id: self.ctx.sender.clone(),
                        timestamp: self.ctx.timestamp,
                    },
                );

                trace!("Adding reaction");
                self.state.items.set(
                    idx,
                    event_item.with_inner_kind(remote_event_item.with_reactions(reactions)),
                );
                self.result.items_updated += 1;
            }
        } else {
            trace!("Timeline item not found, adding reaction to the pending list");
            let EventItemIdentifier::EventId(reaction_event_id) = reaction_id.clone() else {
                error!("Adding local reaction echo to event absent from the timeline");
                return;
            };

            let pending = self.state.reactions.pending.entry(event_id.to_owned()).or_default();

            pending.insert(reaction_event_id);
        }

        if let Flow::Remote { txn_id: Some(txn_id), .. } = &self.ctx.flow {
            let id = EventItemIdentifier::TransactionId(txn_id.clone());
            // Remove the local echo from the reaction map.
            if self.state.reactions.map.remove(&id).is_none() {
                warn!(
                    "Received reaction with transaction ID, but didn't \
                     find matching reaction in reaction_map"
                );
            }
        }
        let reaction_sender_data = ReactionSenderData {
            sender_id: self.ctx.sender.clone(),
            timestamp: self.ctx.timestamp,
        };
        self.state.reactions.map.insert(reaction_id, (reaction_sender_data, c.relates_to));
    }

    #[instrument(skip_all)]
    fn handle_room_encrypted(&mut self, c: RoomEncryptedEventContent) {
        // TODO: Handle replacements if the replaced event is also UTD
        self.add(true, TimelineItemContent::unable_to_decrypt(c));
    }

    // Redacted redactions are no-ops (unfortunately)
    #[instrument(skip_all, fields(redacts_event_id = ?redacts))]
    fn handle_redaction(&mut self, redacts: OwnedEventId, _content: RoomRedactionEventContent) {
        let id = EventItemIdentifier::EventId(redacts.clone());
        if let Some((_, rel)) = self.state.reactions.map.remove(&id) {
            update_timeline_item!(self, &rel.event_id, "redaction", |event_item| {
                let Some(remote_event_item) = event_item.as_remote() else {
                    error!("inconsistent state: redaction received on a non-remote event item");
                    return None;
                };

                let mut reactions = remote_event_item.reactions.clone();

                let count = {
                    let Entry::Occupied(mut group_entry) = reactions.entry(rel.key.clone()) else {
                        return None;
                    };
                    let group = group_entry.get_mut();

                    if group.0.remove(&id).is_none() {
                        error!(
                            "inconsistent state: reaction from reaction_map not in reaction list \
                             of timeline item"
                        );
                        return None;
                    }

                    group.len()
                };

                if count == 0 {
                    reactions.remove(&rel.key);
                }

                trace!("Removing reaction");
                Some(event_item.with_kind(remote_event_item.with_reactions(reactions)))
            });

            if self.result.items_updated == 0 {
                if let Some(reactions) = self.state.reactions.pending.get_mut(&rel.event_id) {
                    if !reactions.remove(&redacts.clone()) {
                        error!(
                            "inconsistent state: reaction from reaction_map not in reaction list \
                             of pending_reactions"
                        );
                    }
                } else {
                    warn!("reaction_map out of sync with timeline items");
                }
            }
        }

        // Even if the event being redacted is a reaction (found in
        // `reaction_map`), it can still be present in the timeline items
        // directly with the raw event timeline feature (not yet implemented).
        update_timeline_item!(self, &redacts, "redaction", |event_item| {
            if event_item.as_remote().is_none() {
                error!("inconsistent state: redaction received on a non-remote event item");
                return None;
            }

            if let TimelineItemContent::RedactedMessage = &event_item.content {
                debug!("event item is already redacted");
                return None;
            }

            Some(event_item.redact(&self.state.room_version))
        });

        if self.result.items_updated == 0 {
            // We will want to know this when debugging redaction issues.
            debug!("redaction affected no event");
        }

        self.state.items.for_each(|mut entry| {
            let Some(event_item) = entry.as_event() else { return };
            let Some(message) = event_item.content.as_message() else { return };
            let Some(in_reply_to) = message.in_reply_to() else { return };
            let TimelineDetails::Ready(replied_to_event) = &in_reply_to.event else { return };
            if redacts == in_reply_to.event_id {
                let replied_to_event = replied_to_event.redact(&self.state.room_version);
                let in_reply_to = InReplyToDetails {
                    event_id: in_reply_to.event_id.clone(),
                    event: TimelineDetails::Ready(Box::new(replied_to_event)),
                };
                let content = TimelineItemContent::Message(message.with_in_reply_to(in_reply_to));
                let new_item = entry.with_kind(event_item.with_content(content, None));

                ObservableVectorEntry::set(&mut entry, new_item);
            }
        });
    }

    // Redacted redactions are no-ops (unfortunately)
    #[instrument(skip_all, fields(redacts_event_id = ?redacts))]
    fn handle_local_redaction(
        &mut self,
        redacts: OwnedTransactionId,
        _content: RoomRedactionEventContent,
    ) {
        let id = EventItemIdentifier::TransactionId(redacts);
        if let Some((_, rel)) = self.state.reactions.map.remove(&id) {
            update_timeline_item!(self, &rel.event_id, "redaction", |event_item| {
                let Some(remote_event_item) = event_item.as_remote() else {
                    error!("inconsistent state: redaction received on a non-remote event item");
                    return None;
                };

                let mut reactions = remote_event_item.reactions.clone();
                let Entry::Occupied(mut group_entry) = reactions.entry(rel.key.clone()) else {
                    return None;
                };
                let group = group_entry.get_mut();

                if group.0.remove(&id).is_none() {
                    error!(
                        "inconsistent state: reaction from reaction_map not in reaction list \
                         of timeline item"
                    );
                    return None;
                }

                if group.len() == 0 {
                    group_entry.remove();
                }

                trace!("Removing reaction");
                Some(event_item.with_kind(remote_event_item.with_reactions(reactions)))
            });
        }

        if self.result.items_updated == 0 {
            // We will want to know this when debugging redaction issues.
            error!("redaction affected no event");
        }
    }

    /// Add a new event item in the timeline.
    fn add(&mut self, should_add: bool, content: TimelineItemContent) {
        if !should_add {
            return;
        }

        self.result.item_added = true;

        let sender = self.ctx.sender.to_owned();
        let sender_profile = TimelineDetails::from_initial_value(self.ctx.sender_profile.clone());
        let timestamp = self.ctx.timestamp;
        let mut reactions = self.pending_reactions().unwrap_or_default();

        let kind: EventTimelineItemKind = match &self.ctx.flow {
            Flow::Local { txn_id } => {
                let send_state = EventSendState::NotSentYet;
                let transaction_id = txn_id.to_owned();
                LocalEventTimelineItem { send_state, transaction_id }
            }
            .into(),
            Flow::Remote { event_id, raw_event, position, .. } => {
                // Drop pending reactions if the message is redacted.
                if let TimelineItemContent::RedactedMessage = content {
                    if !reactions.is_empty() {
                        reactions = BundledReactions::default();
                    }
                }

                let origin = match position {
                    TimelineItemPosition::Start => RemoteEventOrigin::Pagination,
                    // We only paginate backwards for now, so End only happens for syncs
                    TimelineItemPosition::End { from_cache: true } => RemoteEventOrigin::Cache,
                    TimelineItemPosition::End { from_cache: false } => RemoteEventOrigin::Sync,
                    #[cfg(feature = "e2e-encryption")]
                    TimelineItemPosition::Update(idx) => self.state.items[*idx]
                        .as_event()
                        .and_then(|ev| Some(ev.as_remote()?.origin))
                        .unwrap_or_else(|| {
                            error!("Decryption retried on a local event");
                            RemoteEventOrigin::Unknown
                        }),
                };

                RemoteEventTimelineItem {
                    event_id: event_id.clone(),
                    reactions,
                    read_receipts: self.ctx.read_receipts.clone(),
                    is_own: self.ctx.is_own_event,
                    is_highlighted: self.ctx.is_highlighted,
                    encryption_info: self.ctx.encryption_info.clone(),
                    original_json: raw_event.clone(),
                    latest_edit_json: None,
                    origin,
                }
                .into()
            }
        };

        let mut item = EventTimelineItem::new(sender, sender_profile, timestamp, content, kind);

        match &self.ctx.flow {
            Flow::Local { .. } => {
                trace!("Adding new local timeline item");

                // Check if the latest event has the same date as this event.
                if let Some(latest_event) =
                    self.state.items.iter().rev().find_map(|item| item.as_event())
                {
                    let old_ts = latest_event.timestamp();

                    if let Some(day_divider_item) =
                        self.state.maybe_create_day_divider_from_timestamps(old_ts, timestamp)
                    {
                        trace!("Adding day divider (local)");
                        self.state.items.push_back(day_divider_item);
                    }
                } else {
                    // If there is no event item, there is no day divider yet.
                    trace!("Adding first day divider (local)");
                    let day_divider =
                        self.state.new_timeline_item(VirtualTimelineItem::DayDivider(timestamp));
                    self.state.items.push_back(day_divider);
                }

                let item = self.state.new_timeline_item(item);
                self.state.items.push_back(item);
            }

            Flow::Remote { position: TimelineItemPosition::Start, event_id, .. } => {
                if self
                    .state
                    .items
                    .iter()
                    .filter_map(|ev| ev.as_event()?.event_id())
                    .any(|id| id == event_id)
                {
                    trace!("Skipping back-paginated event that has already been seen");
                    return;
                }

                trace!("Adding new remote timeline item at the start");

                // Check if the earliest day divider has the same date as this event.
                if let Some(VirtualTimelineItem::DayDivider(divider_ts)) =
                    self.state.items.front().and_then(|item| item.as_virtual())
                {
                    if let Some(day_divider_item) =
                        self.state.maybe_create_day_divider_from_timestamps(*divider_ts, timestamp)
                    {
                        self.state.items.push_front(day_divider_item);
                    }
                } else {
                    // The list must always start with a day divider.
                    let day_divider =
                        self.state.new_timeline_item(VirtualTimelineItem::DayDivider(timestamp));
                    self.state.items.push_front(day_divider);
                }

                if self.track_read_receipts {
                    maybe_add_implicit_read_receipt(
                        0,
                        &mut item,
                        self.ctx.is_own_event,
                        &mut self.state.items,
                        &mut self.state.users_read_receipts,
                    );
                }

                let item = self.state.new_timeline_item(item);
                self.state.items.insert(1, item);
            }

            Flow::Remote {
                position: TimelineItemPosition::End { .. }, txn_id, event_id, ..
            } => {
                let result = rfind_event_item(&self.state.items, |it| {
                    txn_id.is_some() && it.transaction_id() == txn_id.as_deref()
                        || it.event_id() == Some(event_id)
                });

                let mut removed_event_item_id = None;
                let mut removed_day_divider_id = None;
                if let Some((idx, old_item)) = result {
                    if old_item.as_remote().is_some() {
                        // Item was previously received from the server. This
                        // should be very rare normally, but with the sliding-
                        // sync proxy, it is actually very common.
                        trace!(?item, old_item = ?*old_item, "Received duplicate event");

                        if old_item.content.is_redacted() && !item.content.is_redacted() {
                            warn!("Got original form of an event that was previously redacted");
                            item.content = item.content.redact(&self.state.room_version);
                            item.as_remote_mut()
                                .expect("Can't have a local item when flow == Remote")
                                .reactions
                                .clear();
                        }
                    }

                    if txn_id.is_none() {
                        // The event was created by this client, but the server
                        // sent it back without a transaction ID.
                        warn!("Received remote echo without transaction ID");
                    }

                    // TODO: Check whether anything is different about the
                    //       old and new item?

                    let old_item_id = old_item.internal_id;

                    if idx == self.state.items.len() - 1
                        && timestamp_to_date(old_item.timestamp()) == timestamp_to_date(timestamp)
                    {
                        // If the old item is the last one and no day divider
                        // changes need to happen, replace and return early.

                        if self.track_read_receipts {
                            maybe_add_implicit_read_receipt(
                                idx,
                                &mut item,
                                self.ctx.is_own_event,
                                &mut self.state.items,
                                &mut self.state.users_read_receipts,
                            );
                        }

                        trace!(idx, "Replacing existing event");
                        self.state.items.set(idx, timeline_item(item, old_item_id));
                        return;
                    }

                    // In more complex cases, remove the item and day
                    // divider (if necessary) before re-adding the item.
                    trace!("Removing local echo or duplicate timeline item");
                    removed_event_item_id = Some(self.state.items.remove(idx).internal_id);

                    assert_ne!(
                        idx, 0,
                        "there is never an event item at index 0 because \
                         the first event item is preceded by a day divider"
                    );

                    // Pre-requisites for removing the day divider:
                    // 1. there is one preceding the old item at all
                    if self.state.items[idx - 1].is_day_divider()
                        // 2. the item after the old one that was removed is virtual (it should be
                        //    impossible for this to be a read marker)
                        && self.state
                            .items
                            .get(idx)
                            .map_or(true, |item| item.is_virtual())
                    {
                        trace!("Removing day divider");
                        removed_day_divider_id = Some(self.state.items.remove(idx - 1).internal_id);
                    }

                    // no return here, below code for adding a new event
                    // will run to re-add the removed item
                } else if txn_id.is_some() {
                    warn!(
                        "Received event with transaction ID, but didn't \
                         find matching timeline item"
                    );
                }

                // Local echoes that are pending should stick to the bottom,
                // find the latest event that isn't that.
                let mut latest_event_stream = self
                    .state
                    .items
                    .iter()
                    .enumerate()
                    .rev()
                    .filter_map(|(idx, item)| Some((idx, item.as_event()?)));

                // Find the latest event, independently of success or failure status.
                let latest_event = latest_event_stream.clone().next().unzip().1;

                // Find the index of the latest non-failure event.
                let latest_nonfailed_event_idx = latest_event_stream
                    .find(|(_, evt)| {
                        !matches!(
                            evt.send_state(),
                            Some(EventSendState::NotSentYet | EventSendState::Sent { .. })
                        )
                    })
                    .unzip()
                    .0;

                // Insert the next item after the latest non-failed event item,
                // or at the start if there is no such item.
                let mut insert_idx = latest_nonfailed_event_idx.map_or(0, |idx| idx + 1);

                // Keep push semantics, if we're inserting at the end.
                let should_push = insert_idx == self.state.items.len();

                if let Some(latest_event) = latest_event {
                    // Check if that event has the same date as the new one.
                    let old_ts = latest_event.timestamp();

                    if timestamp_to_date(old_ts) != timestamp_to_date(timestamp) {
                        trace!("Adding day divider (remote)");

                        let id = match removed_day_divider_id {
                            // If a day divider was removed for an item about to be moved and we
                            // now need to add a new one, reuse the previous one's ID.
                            Some(day_divider_id) => day_divider_id,
                            None => self.state.next_internal_id(),
                        };

                        let day_divider_item =
                            timeline_item(VirtualTimelineItem::DayDivider(timestamp), id);

                        if should_push {
                            self.state.items.push_back(day_divider_item);
                        } else {
                            self.state.items.insert(insert_idx, day_divider_item);
                            insert_idx += 1;
                        }
                    }
                } else {
                    // If there is no event item, there is no day divider yet.
                    trace!("Adding first day divider (remote)");
                    let new_day_divider =
                        self.state.new_timeline_item(VirtualTimelineItem::DayDivider(timestamp));
                    if should_push {
                        self.state.items.push_back(new_day_divider);
                    } else {
                        self.state.items.insert(insert_idx, new_day_divider);
                        insert_idx += 1;
                    }
                }

                if self.track_read_receipts {
                    maybe_add_implicit_read_receipt(
                        insert_idx,
                        &mut item,
                        self.ctx.is_own_event,
                        &mut self.state.items,
                        &mut self.state.users_read_receipts,
                    );
                }

                let id = match removed_event_item_id {
                    // If a previous version of the same item (usually a local
                    // echo) was removed and we now need to add it again, reuse
                    // the previous item's ID.
                    Some(id) => id,
                    None => self.state.next_internal_id(),
                };

                trace!("Adding new remote timeline item after all non-pending events");
                let new_item = timeline_item(item, id);
                if should_push {
                    self.state.items.push_back(new_item);
                } else {
                    self.state.items.insert(insert_idx, new_item);
                }
            }

            #[cfg(feature = "e2e-encryption")]
            Flow::Remote { position: TimelineItemPosition::Update(idx), .. } => {
                trace!("Updating timeline item at position {idx}");
                let id = self.state.items[*idx].internal_id;
                self.state.items.set(*idx, timeline_item(item, id));
            }
        }

        // See if we can update the read marker.
        if self.state.event_should_update_fully_read_marker {
            update_read_marker(
                &mut self.state.items,
                self.state.fully_read_event.as_deref(),
                &mut self.state.event_should_update_fully_read_marker,
            );
        }
    }

    fn pending_reactions(&mut self) -> Option<BundledReactions> {
        match &self.ctx.flow {
            Flow::Local { .. } => None,
            Flow::Remote { event_id, .. } => {
                let reactions = self.state.reactions.pending.remove(event_id)?;
                let mut bundled = IndexMap::new();

                for reaction_event_id in reactions {
                    let reaction_id = EventItemIdentifier::EventId(reaction_event_id);
                    let Some((reaction_sender_data, annotation)) =
                        self.state.reactions.map.get(&reaction_id)
                    else {
                        error!(
                            "inconsistent state: reaction from pending_reactions not in reaction_map"
                        );
                        continue;
                    };

                    let group: &mut ReactionGroup =
                        bundled.entry(annotation.key.clone()).or_default();
                    group.0.insert(reaction_id, reaction_sender_data.clone());
                }

                Some(bundled)
            }
        }
    }
}

pub(crate) fn update_read_marker(
    items: &mut ObservableVector<Arc<TimelineItem>>,
    fully_read_event: Option<&EventId>,
    event_should_update_fully_read_marker: &mut bool,
) {
    let Some(fully_read_event) = fully_read_event else { return };
    trace!(?fully_read_event, "Updating read marker");

    let read_marker_idx = find_read_marker(items);
    let fully_read_event_idx = rfind_event_by_id(items, fully_read_event).map(|(idx, _)| idx);

    match (read_marker_idx, fully_read_event_idx) {
        (None, None) => {
            *event_should_update_fully_read_marker = true;
        }
        (None, Some(idx)) => {
            // We don't want to insert the read marker if it is at the end of the timeline.
            if idx + 1 < items.len() {
                *event_should_update_fully_read_marker = false;
                items.insert(idx + 1, TimelineItem::read_marker());
            } else {
                *event_should_update_fully_read_marker = true;
            }
        }
        (Some(_), None) => {
            // Keep the current position of the read marker, hopefully we
            // should have a new position later.
            *event_should_update_fully_read_marker = true;
        }
        (Some(from), Some(to)) => {
            *event_should_update_fully_read_marker = false;

            // The read marker can't move backwards.
            if from < to {
                let item = items.remove(from);

                // We don't want to re-insert the read marker if it is at the end of the
                // timeline.
                if to < items.len() {
                    // Since the fully-read event's index was shifted to the left
                    // by one position by the remove call above, insert the fully-
                    // read marker at its previous position, rather than that + 1
                    items.insert(to, item);
                } else {
                    *event_should_update_fully_read_marker = true;
                }
            }
        }
    }
}

fn _update_timeline_item(
    items: &mut ObservableVector<Arc<TimelineItem>>,
    items_updated: &mut u16,
    event_id: &EventId,
    action: &str,
    update: impl FnOnce(&EventTimelineItem) -> Option<EventTimelineItem>,
) {
    if let Some((idx, item)) = rfind_event_by_id(items, event_id) {
        trace!("Found timeline item to update");
        if let Some(new_item) = update(item.inner) {
            trace!("Updating item");
            items.set(idx, timeline_item(new_item, item.internal_id));
            *items_updated += 1;
        }
    } else {
        debug!("Timeline item not found, discarding {action}");
    }
}
