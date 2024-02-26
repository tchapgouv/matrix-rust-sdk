// Copyright 2024 Mauro Romito
// Copyright 2024 The Matrix.org Foundation C.I.C.
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

use eyeball_im::{ObservableVector, VectorDiff};
use futures_core::Stream;
use ruma::{
    api::client::directory::get_public_rooms_filtered::v3::Request as PublicRoomsFilterRequest,
    directory::{Filter, PublicRoomJoinRule},
    OwnedMxcUri, OwnedRoomAliasId, OwnedRoomId,
};

use crate::Client;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomDescription {
    pub room_id: OwnedRoomId,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub alias: Option<OwnedRoomAliasId>,
    pub avatar_url: Option<OwnedMxcUri>,
    pub join_rule: PublicRoomJoinRule,
    pub is_world_readable: bool,
    pub joined_members: u64,
}

pub struct RoomDirectorySearch {
    batch_size: u32,
    filter: Option<String>,
    next_token: Option<String>,
    client: Client,
    results: ObservableVector<RoomDescription>,
}

impl RoomDirectorySearch {
    pub fn new(client: Client) -> Self {
        Self {
            batch_size: 0,
            filter: None,
            next_token: None,
            client,
            results: ObservableVector::new(),
        }
    }

    pub async fn search(&mut self, filter: Option<String>, batch_size: u32) {
        self.filter = filter;
        self.batch_size = batch_size;
        self.next_token = None;
        self.results.clear();
        self.next_page().await;
    }

    pub async fn next_page(&mut self) {
        let mut filter = Filter::new();
        filter.generic_search_term = self.filter.clone();

        let mut request = PublicRoomsFilterRequest::new();
        request.filter = filter;
        request.limit = Some(self.batch_size.into());
        request.since = self.next_token.clone();
        if let Ok(response) = self.client.public_rooms_filtered(request).await {
            self.next_token = response.next_batch;
            self.results.append(
                response
                    .chunk
                    .into_iter()
                    .map(|room| RoomDescription {
                        room_id: room.room_id,
                        name: room.name,
                        topic: room.topic,
                        alias: room.canonical_alias,
                        avatar_url: room.avatar_url,
                        join_rule: room.join_rule,
                        is_world_readable: room.world_readable,
                        joined_members: room.num_joined_members.into(),
                    })
                    .collect(),
            );
        }
    }

    pub fn results(&self) -> impl Stream<Item = VectorDiff<RoomDescription>> {
        self.results.subscribe().into_stream()
    }
}
