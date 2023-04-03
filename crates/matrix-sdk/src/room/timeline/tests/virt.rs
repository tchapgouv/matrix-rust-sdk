use assert_matches::assert_matches;
use chrono::{Datelike, Local, TimeZone};
use eyeball_im::VectorDiff;
use futures_util::StreamExt;
use matrix_sdk_test::async_test;
use ruma::{
    event_id,
    events::{room::message::RoomMessageEventContent, AnyMessageLikeEventContent},
};

use super::{TestTimeline, ALICE, BOB};
use crate::room::timeline::{TimelineItem, VirtualTimelineItem};

#[async_test]
async fn day_divider() {
    let timeline = TestTimeline::new();
    let mut stream = timeline.subscribe().await;

    timeline
        .handle_live_message_event(
            *ALICE,
            RoomMessageEventContent::text_plain("This is a first message on the first day"),
        )
        .await;

    let day_divider =
        assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    let ts = assert_matches!(
        day_divider.as_virtual().unwrap(),
        VirtualTimelineItem::DayDivider(ts) => *ts
    );
    let date = Local.timestamp_millis_opt(ts.0.into()).single().unwrap();
    assert_eq!(date.year(), 1970);
    assert_eq!(date.month(), 1);
    assert_eq!(date.day(), 1);

    let item = assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    item.as_event().unwrap();

    timeline
        .handle_live_message_event(
            *ALICE,
            RoomMessageEventContent::text_plain("This is a second message on the first day"),
        )
        .await;

    let item = assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    item.as_event().unwrap();

    // Timestamps start at unix epoch, advance to one day later
    timeline.set_next_ts(24 * 60 * 60 * 1000);

    timeline
        .handle_live_message_event(
            *ALICE,
            RoomMessageEventContent::text_plain("This is a first message on the next day"),
        )
        .await;

    let day_divider =
        assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    let ts = assert_matches!(
        day_divider.as_virtual().unwrap(),
        VirtualTimelineItem::DayDivider(ts) => *ts
    );
    let date = Local.timestamp_millis_opt(ts.0.into()).single().unwrap();
    assert_eq!(date.year(), 1970);
    assert_eq!(date.month(), 1);
    assert_eq!(date.day(), 2);

    let item = assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    item.as_event().unwrap();

    let _ = timeline
        .handle_local_event(AnyMessageLikeEventContent::RoomMessage(
            RoomMessageEventContent::text_plain("A message I'm sending just now"),
        ))
        .await;

    // The other events are in the past so a local event always creates a new day
    // divider.
    let day_divider =
        assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    assert_matches!(day_divider.as_virtual().unwrap(), VirtualTimelineItem::DayDivider { .. });

    let item = assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    item.as_event().unwrap();
}

#[async_test]
async fn update_read_marker() {
    let timeline = TestTimeline::new();
    let mut stream = timeline.subscribe().await;

    timeline.handle_live_message_event(&ALICE, RoomMessageEventContent::text_plain("A")).await;
    let _day_divider =
        assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    let item = assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    let first_event_id = item.as_event().unwrap().event_id().unwrap().to_owned();

    timeline.inner.set_fully_read_event(first_event_id.clone()).await;

    // Nothing should happen, the marker cannot be added at the end.

    timeline.handle_live_message_event(&BOB, RoomMessageEventContent::text_plain("B")).await;
    let item = assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    let second_event_id = item.as_event().unwrap().event_id().unwrap().to_owned();

    // Now the read marker appears after the first event.
    let item =
        assert_matches!(stream.next().await, Some(VectorDiff::Insert { index: 2, value }) => value);
    assert_matches!(item.as_virtual(), Some(VirtualTimelineItem::ReadMarker));

    timeline.inner.set_fully_read_event(second_event_id.clone()).await;

    // The read marker is removed but not reinserted, because it cannot be added at
    // the end.
    assert_matches!(stream.next().await, Some(VectorDiff::Remove { index: 2 }));

    timeline.handle_live_message_event(&ALICE, RoomMessageEventContent::text_plain("C")).await;
    let item = assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    let third_event_id = item.as_event().unwrap().event_id().unwrap().to_owned();

    // Now the read marker is reinserted after the second event.
    let marker =
        assert_matches!(stream.next().await, Some(VectorDiff::Insert { index: 3, value }) => value);
    assert_matches!(*marker, TimelineItem::Virtual(VirtualTimelineItem::ReadMarker));

    // Nothing should happen if the fully read event is set back to an older event.
    timeline.inner.set_fully_read_event(first_event_id).await;

    // Nothing should happen if the fully read event isn't found.
    timeline.inner.set_fully_read_event(event_id!("$fake_event_id").to_owned()).await;

    // Nothing should happen if the fully read event is referring to an event
    // that has already been marked as fully read.
    timeline.inner.set_fully_read_event(second_event_id).await;

    timeline.handle_live_message_event(&ALICE, RoomMessageEventContent::text_plain("D")).await;
    let item = assert_matches!(stream.next().await, Some(VectorDiff::PushBack { value }) => value);
    item.as_event().unwrap();

    timeline.inner.set_fully_read_event(third_event_id).await;

    // The read marker is moved after the third event.
    assert_matches!(stream.next().await, Some(VectorDiff::Remove { index: 3 }));
    let marker =
        assert_matches!(stream.next().await, Some(VectorDiff::Insert { index: 4, value }) => value);
    assert_matches!(*marker, TimelineItem::Virtual(VirtualTimelineItem::ReadMarker));
}
