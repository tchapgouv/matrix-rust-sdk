use matrix_sdk::{
    ruma::api::client::room::create_room::v3::Request as CreateRoomRequest, Client, RoomListEntry,
    SlidingSyncBuilder,
};
use matrix_sdk_integration_testing::helpers::get_client_for_user;

#[allow(dead_code)]
async fn setup(name: String) -> anyhow::Result<(Client, SlidingSyncBuilder)> {
    let sliding_sync_proxy_url =
        option_env!("SLIDING_SYNC_PROXY_URL").unwrap_or("http://localhost:8338").to_owned();
    let client = get_client_for_user(name, false).await?;
    let sliding_sync_builder = client
        .sliding_sync()
        .await
        .homeserver(sliding_sync_proxy_url.parse()?)
        .with_common_extensions();
    Ok((client, sliding_sync_builder))
}

#[allow(dead_code)]
async fn random_setup_with_rooms(
    number_of_rooms: usize,
) -> anyhow::Result<(Client, SlidingSyncBuilder)> {
    let namespace = uuid::Uuid::new_v4().to_string();
    let (client, sliding_sync_builder) = setup(namespace.clone()).await?;

    for room_num in 0..number_of_rooms {
        let mut request = CreateRoomRequest::new();
        request.name = Some(format!("{namespace}-{room_num}"));
        let _event_id = client.create_room(request).await?;
    }

    Ok((client, sliding_sync_builder))
}

#[derive(PartialEq, Eq, Debug)]
enum RoomListEntryEasy {
    Empty,
    Invalid,
    Filled,
}

impl From<&RoomListEntry> for RoomListEntryEasy {
    fn from(value: &RoomListEntry) -> Self {
        match value {
            RoomListEntry::Empty => RoomListEntryEasy::Empty,
            RoomListEntry::Invalidated(_) => RoomListEntryEasy::Invalid,
            RoomListEntry::Filled(_) => RoomListEntryEasy::Filled,
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::{pin_mut, stream::StreamExt};
    use matrix_sdk::{
        ruma::events::room::message::RoomMessageEventContent, SlidingSyncMode,
        SlidingSyncViewBuilder,
    };

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn it_works_smoke_test() -> anyhow::Result<()> {
        let (_client, sync_proxy_builder) = setup("odo".to_owned()).await?;
        let sync_proxy = sync_proxy_builder.add_fullsync_view().build().await?;
        let stream = sync_proxy.stream().await?;
        pin_mut!(stream);
        let Some(room_summary ) = stream.next().await else {
            anyhow::bail!("No room summary found, loop ended unsuccessfully");
        };
        let summary = room_summary?;
        assert_eq!(summary.rooms.len(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn adding_view_later() -> anyhow::Result<()> {
        let view_name_1 = "sliding1";
        let view_name_2 = "sliding2";
        let view_name_3 = "sliding3";

        let (client, sync_proxy_builder) = random_setup_with_rooms(20).await?;
        let build_view = |name| {
            SlidingSyncViewBuilder::default()
                .sync_mode(SlidingSyncMode::Selective)
                .set_range(0u32, 10u32)
                .sort(vec!["by_recency".to_string(), "by_name".to_string()])
                .name(name)
                .build()
        };
        let sync_proxy = sync_proxy_builder
            .add_view(build_view(view_name_1)?)
            .add_view(build_view(view_name_2)?)
            .build()
            .await?;
        let Some(view1 )= sync_proxy.view(view_name_1) else {
            anyhow::bail!("but we just added that view!");
        };
        let Some(_view2 )= sync_proxy.view(view_name_2) else {
            anyhow::bail!("but we just added that view!");
        };

        assert!(sync_proxy.view(view_name_3).is_none());

        let stream = sync_proxy.stream().await?;
        pin_mut!(stream);
        let Some(room_summary ) = stream.next().await else {
            anyhow::bail!("No room summary found, loop ended unsuccessfully");
        };
        let summary = room_summary?;
        // we only heard about the ones we had asked for
        assert_eq!(summary.views, [view_name_1, view_name_2]);

        assert!(sync_proxy.add_view(build_view(view_name_3)?).is_none());

        // we need to restart the stream after every view listing update
        let stream = sync_proxy.stream().await?;
        pin_mut!(stream);

        let mut saw_update = false;
        for _n in 0..2 {
            let Some(room_summary ) = stream.next().await else {
                anyhow::bail!("sync has closed unexpectedly");
            };
            let summary = room_summary?;
            // we only heard about the ones we had asked for
            if !summary.views.is_empty() {
                // only if we saw an update come through
                assert_eq!(summary.views, [view_name_3]);
                // we didn't update the other views, so only no 2 should se an update
                saw_update = true;
                break;
            }
        }

        assert!(saw_update, "We didn't see the updae come through the pipe");

        // and let's update the order of all views again
        let Some(RoomListEntry::Filled(room_id)) = view1
            .rooms_list
            .lock_ref()
            .iter().nth(4).map(Clone::clone) else
        {
            anyhow::bail!("4th room has moved? how?");
        };

        let Some(room) = client.get_joined_room(&room_id) else {
            anyhow::bail!("No joined room {room_id}");
        };

        let content = RoomMessageEventContent::text_plain("Hello world");

        room.send(content, None).await?; // this should put our room up to the most recent

        let mut saw_update = false;
        for _n in 0..2 {
            let Some(room_summary ) = stream.next().await else {
                anyhow::bail!("sync has closed unexpectedly");
            };
            let summary = room_summary?;
            // we only heard about the ones we had asked for
            if !summary.views.is_empty() {
                // only if we saw an update come through
                assert_eq!(summary.views, [view_name_1, view_name_2, view_name_3,]);
                // notice that our view 2 is now the last view, but all have seen updates
                saw_update = true;
                break;
            }
        }

        assert!(saw_update, "We didn't see the updae come through the pipe");

        Ok(())
    }

    // index-based views don't support removing views. Leaving this test for an API
    // update later.
    //
    // #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    // async fn live_views() -> anyhow::Result<()> {
    //     let view_name_1 = "sliding1";
    //     let view_name_2 = "sliding2";
    //     let view_name_3 = "sliding3";

    //     let (client, sync_proxy_builder) = random_setup_with_rooms(20).await?;
    //     let build_view = |name| {
    //         SlidingSyncViewBuilder::default()
    //             .sync_mode(SlidingSyncMode::Selective)
    //             .set_range(0u32, 10u32)
    //             .sort(vec!["by_recency".to_string(), "by_name".to_string()])
    //             .name(name)
    //             .build()
    //     };
    //     let sync_proxy = sync_proxy_builder
    //         .add_view(build_view(view_name_1)?)
    //         .add_view(build_view(view_name_2)?)
    //         .add_view(build_view(view_name_3)?)
    //         .build()
    //         .await?;
    //     let Some(view1 )= sync_proxy.view(view_name_1) else {
    //         anyhow::bail!("but we just added that view!");
    //     };
    //     let Some(_view2 )= sync_proxy.view(view_name_2) else {
    //         anyhow::bail!("but we just added that view!");
    //     };

    //     let Some(_view3 )= sync_proxy.view(view_name_3) else {
    //         anyhow::bail!("but we just added that view!");
    //     };

    //     let stream = sync_proxy.stream().await?;
    //     pin_mut!(stream);
    //     let Some(room_summary ) = stream.next().await else {
    //         anyhow::bail!("No room summary found, loop ended unsuccessfully");
    //     };
    //     let summary = room_summary?;
    //     // we only heard about the ones we had asked for
    //     assert_eq!(summary.views, [view_name_1, view_name_2, view_name_3]);

    //     let Some(view_2) = sync_proxy.pop_view(view_name_2) else {
    //         anyhow::bail!("Room exists");
    //     };

    //     // we need to restart the stream after every view listing update
    //     let stream = sync_proxy.stream().await?;
    //     pin_mut!(stream);

    //     // Let's trigger an update by sending a message to room pos=3, making it
    // move to     // pos 0

    //     let Some(RoomListEntry::Filled(room_id)) = view1
    //         .rooms_list
    //         .lock_ref()
    //         .iter().nth(3).map(Clone::clone) else
    //     {
    //         anyhow::bail!("2nd room has moved? how?");
    //     };

    //     let Some(room) = client.get_joined_room(&room_id) else {
    //         anyhow::bail!("No joined room {room_id}");
    //     };

    //     let content = RoomMessageEventContent::text_plain("Hello world");

    //     room.send(content, None).await?; // this should put our room up to the
    // most recent

    //     let mut saw_update = false;
    //     for _n in 0..2 {
    //         let Some(room_summary ) = stream.next().await else {
    //             anyhow::bail!("sync has closed unexpectedly");
    //         };
    //         let summary = room_summary?;
    //         // we only heard about the ones we had asked for
    //         if !summary.views.is_empty() {
    //             // only if we saw an update come through
    //             assert_eq!(summary.views, [view_name_1, view_name_3]);
    //             saw_update = true;
    //             break;
    //         }
    //     }

    //     assert!(saw_update, "We didn't see the updae come through the pipe");

    //     assert!(sync_proxy.add_view(view_2).is_none());

    //     // we need to restart the stream after every view listing update
    //     let stream = sync_proxy.stream().await?;
    //     pin_mut!(stream);

    //     let mut saw_update = false;
    //     for _n in 0..2 {
    //         let Some(room_summary ) = stream.next().await else {
    //             anyhow::bail!("sync has closed unexpectedly");
    //         };
    //         let summary = room_summary?;
    //         // we only heard about the ones we had asked for
    //         if !summary.views.is_empty() {
    //             // only if we saw an update come through
    //             assert_eq!(summary.views, [view_name_2]);
    //             // we didn't update the other views, so only no 2 should se an
    // update             saw_update = true;
    //             break;
    //         }
    //     }

    //     assert!(saw_update, "We didn't see the updae come through the pipe");

    //     // and let's update the order of all views again
    //     let Some(RoomListEntry::Filled(room_id)) = view1
    //         .rooms_list
    //         .lock_ref()
    //         .iter().nth(4).map(Clone::clone) else
    //     {
    //         anyhow::bail!("4th room has moved? how?");
    //     };

    //     let Some(room) = client.get_joined_room(&room_id) else {
    //         anyhow::bail!("No joined room {room_id}");
    //     };

    //     let content = RoomMessageEventContent::text_plain("Hello world");

    //     room.send(content, None).await?; // this should put our room up to the
    // most recent

    //     let mut saw_update = false;
    //     for _n in 0..2 {
    //         let Some(room_summary ) = stream.next().await else {
    //             anyhow::bail!("sync has closed unexpectedly");
    //         };
    //         let summary = room_summary?;
    //         // we only heard about the ones we had asked for
    //         if !summary.views.is_empty() {
    //             // only if we saw an update come through
    //             assert_eq!(summary.views, [view_name_1, view_name_3,
    // view_name_2]);             // notice that our view 2 is now the last
    // view, but all have seen updates             saw_update = true;
    //             break;
    //         }
    //     }

    //     assert!(saw_update, "We didn't see the updae come through the pipe");

    //     Ok(())
    // }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn resizing_sliding_window() -> anyhow::Result<()> {
        let (_client, sync_proxy_builder) = random_setup_with_rooms(20).await?;
        let sliding_window_view = SlidingSyncViewBuilder::default()
            .sync_mode(SlidingSyncMode::Selective)
            .set_range(0u32, 10u32)
            .sort(vec!["by_recency".to_string(), "by_name".to_string()])
            .name("sliding")
            .build()?;
        let sync_proxy = sync_proxy_builder.add_view(sliding_window_view).build().await?;
        let Some(view )= sync_proxy.view("sliding") else {
            anyhow::bail!("but we just added that view!");
        };
        let stream = sync_proxy.stream().await?;
        pin_mut!(stream);
        let Some(room_summary ) = stream.next().await else {
            anyhow::bail!("No room summary found, loop ended unsuccessfully");
        };
        let summary = room_summary?;
        // we only heard about the ones we had asked for
        assert_eq!(summary.rooms.len(), 11);
        let collection_simple = view
            .rooms_list
            .lock_ref()
            .iter()
            .map(Into::<RoomListEntryEasy>::into)
            .collect::<Vec<_>>();
        assert_eq!(
            collection_simple,
            [
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
            ]
        );

        let _signal = view.rooms_list.signal_vec_cloned();

        // let's move the window

        view.set_range(1, 10);
        // Ensure 0-0 invalidation ranges work.

        for _n in 0..2 {
            let Some(room_summary ) = stream.next().await else {
                anyhow::bail!("sync has closed unexpectedly");
            };
            let summary = room_summary?;
            // we only heard about the ones we had asked for
            if summary.views.iter().any(|s| s == "sliding") {
                break;
            }
        }

        let collection_simple = view
            .rooms_list
            .lock_ref()
            .iter()
            .map(Into::<RoomListEntryEasy>::into)
            .collect::<Vec<_>>();
        assert_eq!(
            collection_simple,
            [
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
            ]
        );

        view.set_range(5, 10);

        for _n in 0..2 {
            let Some(room_summary ) = stream.next().await else {
                anyhow::bail!("sync has closed unexpectedly");
            };
            let summary = room_summary?;
            // we only heard about the ones we had asked for
            if summary.views.iter().any(|s| s == "sliding") {
                break;
            }
        }

        let collection_simple = view
            .rooms_list
            .lock_ref()
            .iter()
            .map(Into::<RoomListEntryEasy>::into)
            .collect::<Vec<_>>();
        assert_eq!(
            collection_simple,
            [
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
            ]
        );

        // let's move the window

        view.set_range(5, 15);

        for _n in 0..2 {
            let Some(room_summary ) = stream.next().await else {
                anyhow::bail!("sync has closed unexpectedly");
            };
            let summary = room_summary?;
            // we only heard about the ones we had asked for
            if summary.views.iter().any(|s| s == "sliding") {
                break;
            }
        }

        let collection_simple = view
            .rooms_list
            .lock_ref()
            .iter()
            .map(Into::<RoomListEntryEasy>::into)
            .collect::<Vec<_>>();
        assert_eq!(
            collection_simple,
            [
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
            ]
        );
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn moving_out_of_sliding_window() -> anyhow::Result<()> {
        let (client, sync_proxy_builder) = random_setup_with_rooms(20).await?;
        let sliding_window_view = SlidingSyncViewBuilder::default()
            .sync_mode(SlidingSyncMode::Selective)
            .set_range(1u32, 10u32)
            .sort(vec!["by_recency".to_string(), "by_name".to_string()])
            .name("sliding")
            .build()?;
        let sync_proxy = sync_proxy_builder.add_view(sliding_window_view).build().await?;
        let Some(view )= sync_proxy.view("sliding") else {
            anyhow::bail!("but we just added that view!");
        };
        let stream = sync_proxy.stream().await?;
        pin_mut!(stream);
        let Some(room_summary ) = stream.next().await else {
            anyhow::bail!("No room summary found, loop ended unsuccessfully");
        };
        let summary = room_summary?;
        // we only heard about the ones we had asked for
        assert_eq!(summary.rooms.len(), 10);
        let collection_simple = view
            .rooms_list
            .lock_ref()
            .iter()
            .map(Into::<RoomListEntryEasy>::into)
            .collect::<Vec<_>>();
        assert_eq!(
            collection_simple,
            [
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
            ]
        );

        let _signal = view.rooms_list.signal_vec_cloned();

        // let's move the window

        view.set_range(0, 10);

        for _n in 0..2 {
            let Some(room_summary ) = stream.next().await else {
                anyhow::bail!("sync has closed unexpectedly");
            };
            let summary = room_summary?;
            // we only heard about the ones we had asked for
            if summary.views.iter().any(|s| s == "sliding") {
                break;
            }
        }

        let collection_simple = view
            .rooms_list
            .lock_ref()
            .iter()
            .map(Into::<RoomListEntryEasy>::into)
            .collect::<Vec<_>>();
        assert_eq!(
            collection_simple,
            [
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
            ]
        );

        // let's move the window again

        view.set_range(2, 12);

        for _n in 0..2 {
            let Some(room_summary ) = stream.next().await else {
                anyhow::bail!("sync has closed unexpectedly");
            };
            let summary = room_summary?;
            // we only heard about the ones we had asked for
            if summary.views.iter().any(|s| s == "sliding") {
                break;
            }
        }

        let collection_simple = view
            .rooms_list
            .lock_ref()
            .iter()
            .map(Into::<RoomListEntryEasy>::into)
            .collect::<Vec<_>>();
        assert_eq!(
            collection_simple,
            [
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
            ]
        );

        // now we "move" the room of pos 3 to pos 0;
        // this is a bordering case

        let Some(RoomListEntry::Filled(room_id)) = view
            .rooms_list
            .lock_ref()
            .iter().nth(3).map(Clone::clone) else
        {
            anyhow::bail!("2nd room has moved? how?");
        };

        let Some(room) = client.get_joined_room(&room_id) else {
            anyhow::bail!("No joined room {room_id}");
        };

        let content = RoomMessageEventContent::text_plain("Hello world");

        room.send(content, None).await?; // this should put our room up to the most recent

        for _n in 0..2 {
            let Some(room_summary ) = stream.next().await else {
                anyhow::bail!("sync has closed unexpectedly");
            };
            let summary = room_summary?;
            // we only heard about the ones we had asked for
            if summary.views.iter().any(|s| s == "sliding") {
                break;
            }
        }

        let collection_simple = view
            .rooms_list
            .lock_ref()
            .iter()
            .map(Into::<RoomListEntryEasy>::into)
            .collect::<Vec<_>>();
        assert_eq!(
            collection_simple,
            [
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
            ]
        );

        // items has moved, thus we shouldn't find it where it was
        assert!(
            view.rooms_list.lock_ref().iter().nth(3).unwrap().as_room_id().unwrap() != &room_id
        );

        // let's move the window again

        view.set_range(0, 10);

        for _n in 0..2 {
            let Some(room_summary ) = stream.next().await else {
                anyhow::bail!("sync has closed unexpectedly");
            };
            let summary = room_summary?;
            // we only heard about the ones we had asked for
            if summary.views.iter().any(|s| s == "sliding") {
                break;
            }
        }

        let collection_simple = view
            .rooms_list
            .lock_ref()
            .iter()
            .map(Into::<RoomListEntryEasy>::into)
            .collect::<Vec<_>>();
        assert_eq!(
            collection_simple,
            [
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Filled,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Invalid,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
                RoomListEntryEasy::Empty,
            ]
        );

        // and check that our room move has been accepted properly, too.
        assert_eq!(
            view.rooms_list.lock_ref().iter().next().unwrap().as_room_id().unwrap(),
            &room_id
        );

        Ok(())
    }
}
