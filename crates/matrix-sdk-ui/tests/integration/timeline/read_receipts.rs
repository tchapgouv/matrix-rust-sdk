// Copyright 2023 The Matrix.org Foundation C.I.C.
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

use std::{ops::Not as _, time::Duration};

use assert_matches::assert_matches;
use assert_matches2::assert_let;
use eyeball_im::VectorDiff;
use futures_util::StreamExt;
use matrix_sdk::{
    config::SyncSettings,
    room::Receipts,
    test_utils::{
        logged_in_client_with_server,
        mocks::{MatrixMockServer, RoomMessagesResponseTemplate},
    },
};
use matrix_sdk_test::{
    async_test, event_factory::EventFactory, mocks::mock_encryption_state, sync_timeline_event,
    EphemeralTestEvent, JoinedRoomBuilder, RoomAccountDataTestEvent, SyncResponseBuilder, ALICE,
    BOB, CAROL,
};
use matrix_sdk_ui::timeline::RoomExt;
use ruma::{
    api::client::receipt::create_receipt::v3::ReceiptType,
    event_id,
    events::{
        receipt::ReceiptThread,
        room::message::{MessageType, RoomMessageEventContent, SyncRoomMessageEvent},
        AnySyncMessageLikeEvent, AnySyncTimelineEvent,
    },
    room_id, user_id, RoomVersionId,
};
use serde_json::json;
use stream_assert::{assert_pending, assert_ready};
use tokio::task::yield_now;
use wiremock::{
    matchers::{body_json, header, method, path_regex},
    Mock, ResponseTemplate,
};

use crate::mock_sync;

fn filter_notice(ev: &AnySyncTimelineEvent, _room_version: &RoomVersionId) -> bool {
    match ev {
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
            SyncRoomMessageEvent::Original(msg),
        )) => !matches!(msg.content.msgtype, MessageType::Notice(_)),
        _ => true,
    }
}

#[async_test]
async fn test_read_receipts_updates() {
    let room_id = room_id!("!a98sd12bjh:example.org");
    let (client, server) = logged_in_client_with_server().await;
    let sync_settings = SyncSettings::new().timeout(Duration::from_millis(3000));

    let own_user_id = client.user_id().unwrap();
    let alice = user_id!("@alice:localhost");
    let bob = user_id!("@bob:localhost");

    let second_event_id = event_id!("$e32037280er453l:localhost");
    let third_event_id = event_id!("$Sg2037280074GZr34:localhost");

    let mut sync_builder = SyncResponseBuilder::new();
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    mock_encryption_state(&server, false).await;

    let room = client.get_room(room_id).unwrap();
    let timeline = room.timeline().await.unwrap();
    let (items, mut timeline_stream) = timeline.subscribe().await;
    let mut own_receipts_subscriber = timeline.subscribe_own_user_read_receipts_changed().await;

    assert!(items.is_empty());
    assert_pending!(own_receipts_subscriber);

    let own_receipt = timeline.latest_user_read_receipt(own_user_id).await;
    assert_matches!(own_receipt, None);
    let alice_receipt = timeline.latest_user_read_receipt(alice).await;
    assert_matches!(alice_receipt, None);
    let bob_receipt = timeline.latest_user_read_receipt(bob).await;
    assert_matches!(bob_receipt, None);

    sync_builder.add_joined_room(
        JoinedRoomBuilder::new(room_id)
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "body": "is dancing", "format":
                    "org.matrix.custom.html",
                    "formatted_body": "<strong>is dancing</strong>",
                    "msgtype": "m.text"
                },
                "event_id": "$152037280074GZeOm:localhost",
                "origin_server_ts": 152037280,
                "sender": "@example:localhost",
                "type": "m.room.message",
                "unsigned": {
                    "age": 598971
                }
            }))
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "body": "I'm dancing too",
                    "msgtype": "m.text"
                },
                "event_id": second_event_id,
                "origin_server_ts": 152039280,
                "sender": alice,
                "type": "m.room.message",
            }))
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "body": "Viva la macarena!",
                    "msgtype": "m.text"
                },
                "event_id": third_event_id,
                "origin_server_ts": 152045280,
                "sender": alice,
                "type": "m.room.message",
            })),
    );

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    assert_let!(Some(timeline_updates) = timeline_stream.next().await);
    assert_eq!(timeline_updates.len(), 5);

    // We don't list the read receipt of our own user on events.
    assert_let!(VectorDiff::PushBack { value: first_item } = &timeline_updates[0]);
    let first_event = first_item.as_event().unwrap();
    assert!(first_event.read_receipts().is_empty());

    let (own_receipt_event_id, _) = timeline.latest_user_read_receipt(own_user_id).await.unwrap();
    assert_eq!(own_receipt_event_id, first_event.event_id().unwrap());

    assert_ready!(own_receipts_subscriber);
    assert_pending!(own_receipts_subscriber);

    // Implicit read receipt of @alice:localhost.
    assert_let!(VectorDiff::PushBack { value: second_item } = &timeline_updates[1]);
    let second_event = second_item.as_event().unwrap();
    assert_eq!(second_event.read_receipts().len(), 1);

    // Read receipt of @alice:localhost is moved to third event.
    assert_let!(VectorDiff::Set { index: 1, value: second_item } = &timeline_updates[2]);
    let second_event = second_item.as_event().unwrap();
    assert!(second_event.read_receipts().is_empty());

    assert_let!(VectorDiff::PushBack { value: third_item } = &timeline_updates[3]);
    let third_event = third_item.as_event().unwrap();
    assert_eq!(third_event.read_receipts().len(), 1);

    let (alice_receipt_event_id, _) = timeline.latest_user_read_receipt(alice).await.unwrap();
    assert_eq!(alice_receipt_event_id, third_event_id);

    assert_let!(VectorDiff::PushFront { value: date_divider } = &timeline_updates[4]);
    assert!(date_divider.is_date_divider());

    // Read receipt on unknown event is ignored.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                "$unknowneventid": {
                    "m.read": {
                        alice: {
                            "ts": 1436453550,
                        },
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    let (alice_receipt_event_id, _) = timeline.latest_user_read_receipt(alice).await.unwrap();
    assert_eq!(alice_receipt_event_id, third_event.event_id().unwrap());

    // Read receipt on older event is ignored.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                second_event_id: {
                    "m.read": {
                        alice: {
                            "ts": 1436451550,
                        },
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    let (alice_receipt_event_id, _) = timeline.latest_user_read_receipt(alice).await.unwrap();
    assert_eq!(alice_receipt_event_id, third_event_id);

    // Read receipt on same event is ignored.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                third_event_id: {
                    "m.read": {
                        alice: {
                            "ts": 1436451550,
                        },
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    let (alice_receipt_event_id, _) = timeline.latest_user_read_receipt(alice).await.unwrap();
    assert_eq!(alice_receipt_event_id, third_event_id);

    // New user with explicit threaded and unthreaded read receipts.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                second_event_id: {
                    "m.read": {
                        bob: {
                            "ts": 1436451350,
                        },
                    },
                },
                third_event_id: {
                    "m.read": {
                        bob: {
                            "ts": 1436451550,
                            "thread_id": "main",
                        },
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    assert_let!(Some(timeline_updates) = timeline_stream.next().await);
    assert_eq!(timeline_updates.len(), 1);

    assert_let!(VectorDiff::Set { index: 3, value: third_item } = &timeline_updates[0]);
    let third_event = third_item.as_event().unwrap();
    assert_eq!(third_event.read_receipts().len(), 2);

    let (bob_receipt_event_id, _) = timeline.latest_user_read_receipt(bob).await.unwrap();
    assert_eq!(bob_receipt_event_id, third_event_id);

    assert_pending!(own_receipts_subscriber);

    // Private read receipt is updated.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                second_event_id: {
                    "m.read.private": {
                        own_user_id: {
                            "ts": 1436453550,
                        },
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    let (own_user_receipt_event_id, _) =
        timeline.latest_user_read_receipt(own_user_id).await.unwrap();
    assert_eq!(own_user_receipt_event_id, second_event_id);

    assert_ready!(own_receipts_subscriber);
    assert_pending!(own_receipts_subscriber);
    assert_pending!(timeline_stream);
}

#[async_test]
async fn test_read_receipts_updates_on_filtered_events() {
    let room_id = room_id!("!a98sd12bjh:example.org");
    let (client, server) = logged_in_client_with_server().await;
    let sync_settings = SyncSettings::new().timeout(Duration::from_millis(3000));

    let own_user_id = client.user_id().unwrap();

    let event_a_id = event_id!("$152037280074GZeOm:localhost");
    let event_b_id = event_id!("$e32037280er453l:localhost");
    let event_c_id = event_id!("$Sg2037280074GZr34:localhost");

    let mut sync_builder = SyncResponseBuilder::new();
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    mock_encryption_state(&server, false).await;

    let room = client.get_room(room_id).unwrap();
    let timeline = room.timeline_builder().event_filter(filter_notice).build().await.unwrap();
    let (items, mut timeline_stream) = timeline.subscribe().await;

    assert!(items.is_empty());

    let own_receipt = timeline.latest_user_read_receipt(own_user_id).await;
    assert_matches!(own_receipt, None);
    let own_receipt_timeline_event =
        timeline.latest_user_read_receipt_timeline_event_id(own_user_id).await;
    assert_matches!(own_receipt_timeline_event, None);
    let alice_receipt = timeline.latest_user_read_receipt(*ALICE).await;
    assert_matches!(alice_receipt, None);
    let alice_receipt_timeline_event =
        timeline.latest_user_read_receipt_timeline_event_id(*ALICE).await;
    assert_matches!(alice_receipt_timeline_event, None);
    let bob_receipt = timeline.latest_user_read_receipt(*BOB).await;
    assert_matches!(bob_receipt, None);
    let bob_receipt_timeline_event =
        timeline.latest_user_read_receipt_timeline_event_id(*BOB).await;
    assert_matches!(bob_receipt_timeline_event, None);

    sync_builder.add_joined_room(
        JoinedRoomBuilder::new(room_id)
            // Event A
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "body": "is dancing",
                    "msgtype": "m.text"
                },
                "event_id": event_a_id,
                "origin_server_ts": 152037280,
                "sender": own_user_id,
                "type": "m.room.message",
            }))
            // Event B
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "body": "I'm dancing too",
                    "msgtype": "m.notice"
                },
                "event_id": event_b_id,
                "origin_server_ts": 152039280,
                "sender": *BOB,
                "type": "m.room.message",
            }))
            // Event C
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "body": "Viva la macarena!",
                    "msgtype": "m.text"
                },
                "event_id": event_c_id,
                "origin_server_ts": 152045280,
                "sender": *ALICE,
                "type": "m.room.message",
            })),
    );

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    assert_let!(Some(timeline_updates) = timeline_stream.next().await);
    assert_eq!(timeline_updates.len(), 4);

    // We don't list the read receipt of our own user on events.
    assert_let!(VectorDiff::PushBack { value: item_a } = &timeline_updates[0]);
    let event_a = item_a.as_event().unwrap();
    assert!(event_a.read_receipts().is_empty());

    let (own_receipt_event_id, _) = timeline.latest_user_read_receipt(own_user_id).await.unwrap();
    assert_eq!(own_receipt_event_id, event_a_id);
    let own_receipt_timeline_event =
        timeline.latest_user_read_receipt_timeline_event_id(own_user_id).await.unwrap();
    assert_eq!(own_receipt_timeline_event, event_a_id);

    // Implicit read receipt of @bob:localhost.
    assert_let!(VectorDiff::Set { index: 0, value: item_a } = &timeline_updates[1]);
    let event_a = item_a.as_event().unwrap();
    assert_eq!(event_a.read_receipts().len(), 1);

    // Real receipt is on event B.
    let (bob_receipt_event_id, _) = timeline.latest_user_read_receipt(*BOB).await.unwrap();
    assert_eq!(bob_receipt_event_id, event_b_id);
    // Visible receipt is on event A.
    let bob_receipt_timeline_event =
        timeline.latest_user_read_receipt_timeline_event_id(*BOB).await.unwrap();
    assert_eq!(bob_receipt_timeline_event, event_a.event_id().unwrap());

    // Implicit read receipt of @alice:localhost.
    assert_let!(VectorDiff::PushBack { value: item_c } = &timeline_updates[2]);
    let event_c = item_c.as_event().unwrap();
    assert_eq!(event_c.read_receipts().len(), 1);

    let (alice_receipt_event_id, _) = timeline.latest_user_read_receipt(*ALICE).await.unwrap();
    assert_eq!(alice_receipt_event_id, event_c_id);
    let alice_receipt_timeline_event =
        timeline.latest_user_read_receipt_timeline_event_id(*ALICE).await.unwrap();
    assert_eq!(alice_receipt_timeline_event, event_c_id);

    assert_let!(VectorDiff::PushFront { value: date_divider } = &timeline_updates[3]);
    assert!(date_divider.is_date_divider());

    // Read receipt on filtered event.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                event_b_id: {
                    "m.read": {
                        own_user_id: {
                            "ts": 1436451550,
                        },
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    // Real receipt changed to event B.
    let (own_receipt_event_id, _) = timeline.latest_user_read_receipt(own_user_id).await.unwrap();
    assert_eq!(own_receipt_event_id, event_b_id);
    // Visible receipt is still on event A.
    let own_receipt_timeline_event =
        timeline.latest_user_read_receipt_timeline_event_id(own_user_id).await.unwrap();
    assert_eq!(own_receipt_timeline_event, event_a.event_id().unwrap());

    // Update with explicit read receipt.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                event_c_id: {
                    "m.read": {
                        *BOB: {
                            "ts": 1436451550,
                        },
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    assert_let!(Some(timeline_updates) = timeline_stream.next().await);
    assert_eq!(timeline_updates.len(), 2);

    assert_let!(VectorDiff::Set { index: 1, value: item_a } = &timeline_updates[0]);
    let event_a = item_a.as_event().unwrap();
    assert!(event_a.read_receipts().is_empty());

    assert_let!(VectorDiff::Set { index: 2, value: item_c } = &timeline_updates[1]);
    let event_c = item_c.as_event().unwrap();
    assert_eq!(event_c.read_receipts().len(), 2);

    // Both real and visible receipts are now on event C.
    let (bob_receipt_event_id, _) = timeline.latest_user_read_receipt(*BOB).await.unwrap();
    assert_eq!(bob_receipt_event_id, event_c_id);
    let bob_receipt_timeline_event =
        timeline.latest_user_read_receipt_timeline_event_id(*BOB).await.unwrap();
    assert_eq!(bob_receipt_timeline_event, event_c_id);

    // Private read receipt is updated.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                event_c_id: {
                    "m.read.private": {
                        own_user_id: {
                            "ts": 1436453550,
                        },
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    // Both real and visible receipts are now on event C.
    let (own_user_receipt_event_id, _) =
        timeline.latest_user_read_receipt(own_user_id).await.unwrap();
    assert_eq!(own_user_receipt_event_id, event_c_id);
    let own_receipt_timeline_event =
        timeline.latest_user_read_receipt_timeline_event_id(own_user_id).await.unwrap();
    assert_eq!(own_receipt_timeline_event, event_c_id);
    assert_pending!(timeline_stream);
}

#[async_test]
async fn test_send_single_receipt() {
    let room_id = room_id!("!a98sd12bjh:example.org");
    let (client, server) = logged_in_client_with_server().await;
    let sync_settings = SyncSettings::new().timeout(Duration::from_millis(3000));

    let own_user_id = client.user_id().unwrap();

    let mut sync_builder = SyncResponseBuilder::new();
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    mock_encryption_state(&server, false).await;

    let room = client.get_room(room_id).unwrap();
    let timeline = room.timeline().await.unwrap();

    // Unknown receipts are sent.
    let first_receipts_event_id = event_id!("$first_receipts_event_id");

    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/receipt/m\.read/"))
        .and(header("authorization", "Bearer 1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .named("Public read receipt")
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/receipt/m\.read\.private/"))
        .and(header("authorization", "Bearer 1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .named("Private read receipt")
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/receipt/m\.fully_read/"))
        .and(header("authorization", "Bearer 1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .named("Fully-read marker")
        .mount(&server)
        .await;

    timeline
        .send_single_receipt(
            ReceiptType::Read,
            ReceiptThread::Unthreaded,
            first_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    timeline
        .send_single_receipt(
            ReceiptType::ReadPrivate,
            ReceiptThread::Unthreaded,
            first_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    timeline
        .send_single_receipt(
            ReceiptType::FullyRead,
            ReceiptThread::Unthreaded,
            first_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    server.reset().await;

    // Unchanged receipts are not sent.
    sync_builder.add_joined_room(
        JoinedRoomBuilder::new(room_id)
            .add_ephemeral_event(EphemeralTestEvent::Custom(json!({
                "content": {
                    first_receipts_event_id: {
                        "m.read.private": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                        "m.read": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                    },
                },
                "type": "m.receipt",
            })))
            .add_account_data(RoomAccountDataTestEvent::Custom(json!({
                "content": {
                    "event_id": first_receipts_event_id,
                },
                "type": "m.fully_read",
            }))),
    );

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    timeline
        .send_single_receipt(
            ReceiptType::Read,
            ReceiptThread::Unthreaded,
            first_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    timeline
        .send_single_receipt(
            ReceiptType::ReadPrivate,
            ReceiptThread::Unthreaded,
            first_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    timeline
        .send_single_receipt(
            ReceiptType::FullyRead,
            ReceiptThread::Unthreaded,
            first_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    server.reset().await;

    // Receipts with unknown previous receipts are always sent.
    let second_receipts_event_id = event_id!("$second_receipts_event_id");

    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/receipt/m\.read/"))
        .and(header("authorization", "Bearer 1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .named("Public read receipt")
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/receipt/m\.read\.private/"))
        .and(header("authorization", "Bearer 1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .named("Private read receipt")
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/receipt/m\.fully_read/"))
        .and(header("authorization", "Bearer 1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .named("Fully-read marker")
        .mount(&server)
        .await;

    timeline
        .send_single_receipt(
            ReceiptType::Read,
            ReceiptThread::Unthreaded,
            second_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    timeline
        .send_single_receipt(
            ReceiptType::ReadPrivate,
            ReceiptThread::Unthreaded,
            second_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    timeline
        .send_single_receipt(
            ReceiptType::FullyRead,
            ReceiptThread::Unthreaded,
            second_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    server.reset().await;

    // Newer receipts in the timeline are sent.
    let third_receipts_event_id = event_id!("$third_receipts_event_id");

    sync_builder.add_joined_room(
        JoinedRoomBuilder::new(room_id)
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "body": "I'm User A",
                    "msgtype": "m.text",
                },
                "event_id": second_receipts_event_id,
                "origin_server_ts": 152046694,
                "sender": "@user_a:example.org",
                "type": "m.room.message",
            }))
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "body": "I'm User B",
                    "msgtype": "m.text",
                },
                "event_id": third_receipts_event_id,
                "origin_server_ts": 152049794,
                "sender": "@user_b:example.org",
                "type": "m.room.message",
            }))
            .add_ephemeral_event(EphemeralTestEvent::Custom(json!({
                "content": {
                    second_receipts_event_id: {
                        "m.read.private": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                        "m.read": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                    },
                },
                "type": "m.receipt",
            })))
            .add_account_data(RoomAccountDataTestEvent::Custom(json!({
                "content": {
                    "event_id": second_receipts_event_id,
                },
                "type": "m.fully_read",
            }))),
    );

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/receipt/m\.read/"))
        .and(header("authorization", "Bearer 1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .named("Public read receipt")
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/receipt/m\.read\.private/"))
        .and(header("authorization", "Bearer 1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .named("Private read receipt")
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/receipt/m\.fully_read/"))
        .and(header("authorization", "Bearer 1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .named("Fully-read marker")
        .mount(&server)
        .await;

    timeline
        .send_single_receipt(
            ReceiptType::Read,
            ReceiptThread::Unthreaded,
            third_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    timeline
        .send_single_receipt(
            ReceiptType::ReadPrivate,
            ReceiptThread::Unthreaded,
            third_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    timeline
        .send_single_receipt(
            ReceiptType::FullyRead,
            ReceiptThread::Unthreaded,
            third_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    server.reset().await;

    // Older receipts in the timeline are not sent.
    sync_builder.add_joined_room(
        JoinedRoomBuilder::new(room_id)
            .add_ephemeral_event(EphemeralTestEvent::Custom(json!({
                "content": {
                    third_receipts_event_id: {
                        "m.read.private": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                        "m.read": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                    },
                },
                "type": "m.receipt",
            })))
            .add_account_data(RoomAccountDataTestEvent::Custom(json!({
                "content": {
                    "event_id": third_receipts_event_id,
                },
                "type": "m.fully_read",
            }))),
    );

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    timeline
        .send_single_receipt(
            ReceiptType::Read,
            ReceiptThread::Unthreaded,
            second_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    timeline
        .send_single_receipt(
            ReceiptType::ReadPrivate,
            ReceiptThread::Unthreaded,
            second_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
    timeline
        .send_single_receipt(
            ReceiptType::FullyRead,
            ReceiptThread::Unthreaded,
            second_receipts_event_id.to_owned(),
        )
        .await
        .unwrap();
}

#[async_test]
async fn test_mark_as_read() {
    let room_id = room_id!("!a98sd12bjh:example.org");
    let (client, server) = logged_in_client_with_server().await;
    let sync_settings = SyncSettings::new().timeout(Duration::from_millis(3000));

    let own_user_id = client.user_id().unwrap();

    let mut sync_builder = SyncResponseBuilder::new();
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    mock_encryption_state(&server, false).await;

    let room = client.get_room(room_id).unwrap();
    let timeline = room.timeline().await.unwrap();

    let original_event_id = event_id!("$original_event_id");
    let reaction_event_id = event_id!("$reaction_event_id");

    // When I receive an event with a reaction on it,
    sync_builder.add_joined_room(
        JoinedRoomBuilder::new(room_id)
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "body": "I like big Rust and I cannot lie",
                    "msgtype": "m.text",
                },
                "event_id": original_event_id,
                "origin_server_ts": 152046694,
                "sender": "@sir-axalot:example.org",
                "type": "m.room.message",
            }))
            .add_ephemeral_event(EphemeralTestEvent::Custom(json!({
                "content": {
                    original_event_id: {
                        "m.read": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                    },
                },
                "type": "m.receipt",
            })))
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "m.relates_to": {
                        "event_id": original_event_id,
                        "key": "🔥🔥🔥",
                        "rel_type": "m.annotation",
                    },
                },
                "event_id": reaction_event_id,
                "origin_server_ts": 152038300,
                "sender": "@prime-minirusta:example.org",
                "type": "m.reaction",
            })),
    );

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    // And I try to mark the latest event related to a timeline item as read,
    let latest_event = timeline.latest_event().await.expect("missing timeline event item");
    let latest_event_id =
        latest_event.event_id().expect("missing event id for latest timeline event item");

    let has_sent = timeline
        .send_single_receipt(
            ReceiptType::Read,
            ReceiptThread::Unthreaded,
            latest_event_id.to_owned(),
        )
        .await
        .unwrap();

    // Then no request is actually sent, because the event forming the timeline item
    // (the message) is known as read.
    assert!(has_sent.not());
    server.reset().await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/receipt/m\.read/"))
        .and(header("authorization", "Bearer 1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .named("Public read receipt")
        .mount(&server)
        .await;

    // But when I mark the room as read by sending a read receipt to the latest
    // event,
    let has_sent = timeline.mark_as_read(ReceiptType::Read).await.unwrap();

    // It works.
    assert!(has_sent);

    server.reset().await;
}

#[async_test]
async fn test_send_multiple_receipts() {
    let room_id = room_id!("!a98sd12bjh:example.org");
    let (client, server) = logged_in_client_with_server().await;
    let sync_settings = SyncSettings::new().timeout(Duration::from_millis(3000));

    let own_user_id = client.user_id().unwrap();

    let mut sync_builder = SyncResponseBuilder::new();
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    mock_encryption_state(&server, false).await;

    let room = client.get_room(room_id).unwrap();
    let timeline = room.timeline().await.unwrap();

    // Unknown receipts are sent.
    let first_receipts_event_id = event_id!("$first_receipts_event_id");
    let first_receipts = Receipts::new()
        .fully_read_marker(Some(first_receipts_event_id.to_owned()))
        .public_read_receipt(Some(first_receipts_event_id.to_owned()))
        .private_read_receipt(Some(first_receipts_event_id.to_owned()));

    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/read_markers$"))
        .and(header("authorization", "Bearer 1234"))
        .and(body_json(json!({
            "m.fully_read": first_receipts_event_id,
            "m.read": first_receipts_event_id,
            "m.read.private": first_receipts_event_id,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    timeline.send_multiple_receipts(first_receipts.clone()).await.unwrap();
    server.reset().await;

    // Unchanged receipts are not sent.
    sync_builder.add_joined_room(
        JoinedRoomBuilder::new(room_id)
            .add_ephemeral_event(EphemeralTestEvent::Custom(json!({
                "content": {
                    first_receipts_event_id: {
                        "m.read.private": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                        "m.read": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                    },
                },
                "type": "m.receipt",
            })))
            .add_account_data(RoomAccountDataTestEvent::Custom(json!({
                "content": {
                    "event_id": first_receipts_event_id,
                },
                "type": "m.fully_read",
            }))),
    );

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    timeline.send_multiple_receipts(first_receipts).await.unwrap();
    server.reset().await;

    // Receipts with unknown previous receipts are always sent.
    let second_receipts_event_id = event_id!("$second_receipts_event_id");
    let second_receipts = Receipts::new()
        .fully_read_marker(Some(second_receipts_event_id.to_owned()))
        .public_read_receipt(Some(second_receipts_event_id.to_owned()))
        .private_read_receipt(Some(second_receipts_event_id.to_owned()));

    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/read_markers$"))
        .and(header("authorization", "Bearer 1234"))
        .and(body_json(json!({
            "m.fully_read": second_receipts_event_id,
            "m.read": second_receipts_event_id,
            "m.read.private": second_receipts_event_id,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    timeline.send_multiple_receipts(second_receipts.clone()).await.unwrap();
    server.reset().await;

    // Newer receipts in the timeline are sent.
    let third_receipts_event_id = event_id!("$third_receipts_event_id");
    let third_receipts = Receipts::new()
        .fully_read_marker(Some(third_receipts_event_id.to_owned()))
        .public_read_receipt(Some(third_receipts_event_id.to_owned()))
        .private_read_receipt(Some(third_receipts_event_id.to_owned()));

    sync_builder.add_joined_room(
        JoinedRoomBuilder::new(room_id)
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "body": "I'm User A",
                    "msgtype": "m.text",
                },
                "event_id": second_receipts_event_id,
                "origin_server_ts": 152046694,
                "sender": "@user_a:example.org",
                "type": "m.room.message",
            }))
            .add_timeline_event(sync_timeline_event!({
                "content": {
                    "body": "I'm User B",
                    "msgtype": "m.text",
                },
                "event_id": third_receipts_event_id,
                "origin_server_ts": 152049794,
                "sender": "@user_b:example.org",
                "type": "m.room.message",
            }))
            .add_ephemeral_event(EphemeralTestEvent::Custom(json!({
                "content": {
                    second_receipts_event_id: {
                        "m.read.private": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                        "m.read": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                    },
                },
                "type": "m.receipt",
            })))
            .add_account_data(RoomAccountDataTestEvent::Custom(json!({
                "content": {
                    "event_id": second_receipts_event_id,
                },
                "type": "m.fully_read",
            }))),
    );

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/_matrix/client/r0/rooms/.*/read_markers$"))
        .and(header("authorization", "Bearer 1234"))
        .and(body_json(json!({
            "m.fully_read": third_receipts_event_id,
            "m.read": third_receipts_event_id,
            "m.read.private": third_receipts_event_id,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    timeline.send_multiple_receipts(third_receipts.clone()).await.unwrap();
    server.reset().await;

    // Older receipts in the timeline are not sent.
    sync_builder.add_joined_room(
        JoinedRoomBuilder::new(room_id)
            .add_ephemeral_event(EphemeralTestEvent::Custom(json!({
                "content": {
                    third_receipts_event_id: {
                        "m.read.private": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                        "m.read": {
                            own_user_id: {
                                "ts": 1436453550,
                            },
                        },
                    },
                },
                "type": "m.receipt",
            })))
            .add_account_data(RoomAccountDataTestEvent::Custom(json!({
                "content": {
                    "event_id": third_receipts_event_id,
                },
                "type": "m.fully_read",
            }))),
    );

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    timeline.send_multiple_receipts(second_receipts.clone()).await.unwrap();
}

#[async_test]
async fn test_latest_user_read_receipt() {
    let room_id = room_id!("!a98sd12bjh:example.org");
    let (client, server) = logged_in_client_with_server().await;
    let sync_settings = SyncSettings::new().timeout(Duration::from_millis(3000));

    let own_user_id = client.user_id().unwrap();

    let event_a_id = event_id!("$event_a");
    let event_b_id = event_id!("$event_b");
    let event_c_id = event_id!("$event_c");
    let event_d_id = event_id!("$event_d");
    let event_e_id = event_id!("$event_e");

    let mut sync_builder = SyncResponseBuilder::new();
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    mock_encryption_state(&server, false).await;

    let room = client.get_room(room_id).unwrap();
    let timeline = room.timeline().await.unwrap();
    let (items, _) = timeline.subscribe().await;

    assert!(items.is_empty());

    let user_receipt = timeline.latest_user_read_receipt(own_user_id).await;
    assert_matches!(user_receipt, None);

    // Only private receipt.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                event_a_id: {
                    "m.read.private": {
                        own_user_id: {},
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    let (user_receipt_id, _) = timeline.latest_user_read_receipt(own_user_id).await.unwrap();
    assert_eq!(user_receipt_id, event_a_id);

    // Private and public receipts without timestamp should return private
    // receipt.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                event_b_id: {
                    "m.read": {
                        own_user_id: {},
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    let (user_receipt_id, _) = timeline.latest_user_read_receipt(own_user_id).await.unwrap();
    assert_eq!(user_receipt_id, event_a_id);

    // Public receipt with bigger timestamp.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                event_c_id: {
                    "m.read.private": {
                        own_user_id: {
                            "ts": 1,
                        },
                    },
                },
                event_d_id: {
                    "m.read": {
                        own_user_id: {
                            "ts": 10,
                        },
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    let (user_receipt_id, _) = timeline.latest_user_read_receipt(own_user_id).await.unwrap();
    assert_eq!(user_receipt_id, event_d_id);

    // Private receipt with bigger timestamp.
    sync_builder.add_joined_room(JoinedRoomBuilder::new(room_id).add_ephemeral_event(
        EphemeralTestEvent::Custom(json!({
            "content": {
                event_e_id: {
                    "m.read.private": {
                        own_user_id: {
                            "ts": 100,
                        },
                    },
                },
            },
            "type": "m.receipt",
        })),
    ));

    mock_sync(&server, sync_builder.build_json_sync_response(), None).await;
    let _response = client.sync_once(sync_settings.clone()).await.unwrap();
    server.reset().await;

    let (user_receipt_id, _) = timeline.latest_user_read_receipt(own_user_id).await.unwrap();
    assert_eq!(user_receipt_id, event_e_id);
}

#[async_test]
async fn test_no_duplicate_receipt_after_backpagination() {
    let server = MatrixMockServer::new().await;
    let client = server.client_builder().build().await;

    client.event_cache().subscribe().unwrap();

    let room_id = room_id!("!a98sd12bjh:example.org");

    // We want the following final state in the room:
    // - received from back-pagination:
    //  - $1: an event from Alice
    //  - $2: an event from Bob
    // - received from sync:
    //  - $3: an hidden event sent by Alice, with a read receipt from Carol
    //
    // As a result, since $3 is *after* the two others, Alice's implicit read
    // receipt and Carol's receipt on the edit event should be placed onto the
    // most recent rendered event, that is, $2.

    let eid1 = event_id!("$1_backpaginated_oldest");
    let eid2 = event_id!("$2_backpaginated_newest");
    let eid3 = event_id!("$3_sync_event");

    let f = EventFactory::new().room(room_id);

    // Alice sends an edit via sync.
    let ev3 = f
        .text_msg("* I am Alice.")
        .edit(eid1, RoomMessageEventContent::text_plain("I am Alice.").into())
        .sender(*ALICE)
        .event_id(eid3)
        .into_raw_sync();

    // Carol has a read receipt on the edit.
    let read_receipt_event_content = f
        .read_receipts()
        .add(eid3, *CAROL, ruma::events::receipt::ReceiptType::Read, ReceiptThread::Unthreaded)
        .build();

    let prev_batch_token = "prev-batch-token";

    let room = server
        .sync_room(
            &client,
            JoinedRoomBuilder::new(room_id)
                .add_timeline_event(ev3)
                .add_ephemeral_event(EphemeralTestEvent::Custom(json!({
                    "type": "m.receipt",
                    "room_id": room_id,
                    "content": read_receipt_event_content,
                })))
                .set_timeline_limited()
                .set_timeline_prev_batch(prev_batch_token),
        )
        .await;

    let timeline = room.timeline().await.unwrap();

    server
        .mock_room_messages()
        .match_from(prev_batch_token)
        .ok(RoomMessagesResponseTemplate::default().events(vec![
            // In reverse order!
            f.text_msg("I am Bob.").sender(*BOB).event_id(eid2),
            f.text_msg("I am the destroyer of worlds.").sender(*ALICE).event_id(eid1),
        ]))
        .mock_once()
        .mount()
        .await;

    timeline.paginate_backwards(42).await.unwrap();

    yield_now().await;

    // Check that the receipts are at the correct place.
    let timeline_items = timeline.items().await;
    assert_eq!(timeline_items.len(), 3);

    assert!(timeline_items[0].is_date_divider());

    {
        let event1 = timeline_items[1].as_event().unwrap();
        // Sanity check: this is the edited event from Alice.
        assert_eq!(event1.event_id().unwrap(), eid1);

        let receipts = &event1.read_receipts();

        // Carol has explicitly seen ev3, which is after Bob's event, so there shouldn't
        // be a receipt for them here.
        assert!(receipts.get(*CAROL).is_none());

        // Alice has seen this event, being the sender; but Alice has also sent an edit
        // after Bob's message, so Alice must not have a read receipt here.
        assert!(receipts.get(*ALICE).is_none());

        // And Bob has seen the original, but posted something after it, so no receipt
        // for Bob either.
        assert!(receipts.get(*BOB).is_none());

        // In other words, no receipts here.
        assert!(receipts.is_empty());
    }

    {
        let event2 = timeline_items[2].as_event().unwrap();

        // Sanity check: this is Bob's event.
        assert_eq!(event2.event_id().unwrap(), eid2);

        let receipts = &event2.read_receipts();
        // Bob's event should hold *all* the receipts:
        assert_eq!(receipts.len(), 3);
        receipts.get(*ALICE).unwrap();
        receipts.get(*BOB).unwrap();
        receipts.get(*CAROL).unwrap();
    }
}
