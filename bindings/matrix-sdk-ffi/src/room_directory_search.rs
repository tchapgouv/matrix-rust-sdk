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

use std::{fmt::Debug, sync::Arc};

use eyeball_im::VectorDiff;
use futures_util::pin_mut;
use matrix_sdk::room_directory_search::RoomDirectorySearch as SdkRoomDirectorySearch;
use tokio::sync::RwLock;

use super::RUNTIME;
use crate::{error::ClientError, task_handle::TaskHandle};

#[derive(uniffi::Enum)]
pub enum PublicRoomJoinRule {
    Public,
    Knock,
}

impl PublicRoomJoinRule {
    fn convert(value: ruma::directory::PublicRoomJoinRule) -> Option<Self> {
        match value {
            ruma::directory::PublicRoomJoinRule::Public => Some(Self::Public),
            ruma::directory::PublicRoomJoinRule::Knock => Some(Self::Knock),
            _ => None,
        }
    }
}

#[derive(uniffi::Record)]
pub struct RoomDescription {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub alias: Option<String>,
    pub avatar_url: Option<String>,
    pub join_rule: Option<PublicRoomJoinRule>,
    pub is_world_readable: bool,
    pub joined_members: u64,
}

impl From<matrix_sdk::room_directory_search::RoomDescription> for RoomDescription {
    fn from(value: matrix_sdk::room_directory_search::RoomDescription) -> Self {
        Self {
            room_id: value.room_id.to_string(),
            name: value.name,
            topic: value.topic,
            alias: value.alias.map(|alias| alias.to_string()),
            avatar_url: value.avatar_url.map(|url| url.to_string()),
            join_rule: PublicRoomJoinRule::convert(value.join_rule),
            is_world_readable: value.is_world_readable,
            joined_members: value.joined_members,
        }
    }
}

#[derive(uniffi::Object)]
pub struct RoomDirectorySearch {
    pub(crate) inner: RwLock<SdkRoomDirectorySearch>,
}

impl RoomDirectorySearch {
    pub fn new(inner: SdkRoomDirectorySearch) -> Self {
        Self { inner: RwLock::new(inner) }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl RoomDirectorySearch {
    pub async fn next_page(&self) -> Result<(), ClientError> {
        let mut inner = self.inner.write().await;
        inner.next_page().await?;
        Ok(())
    }

    pub async fn search(&self, filter: Option<String>, batch_size: u32) -> Result<(), ClientError> {
        let mut inner = self.inner.write().await;
        inner.search(filter, batch_size).await?;
        Ok(())
    }

    pub async fn loaded_pages(&self) -> Result<u32, ClientError> {
        let inner = self.inner.read().await;
        Ok(inner.loaded_pages() as u32)
    }

    pub async fn is_at_last_page(&self) -> Result<bool, ClientError> {
        let inner = self.inner.read().await;
        Ok(inner.is_at_last_page())
    }

    pub async fn results(
        &self,
        listener: Box<dyn RoomDirectorySearchEntriesListener>,
    ) -> TaskHandle {
        let (initial_values, stream) = self.inner.read().await.results();
        
        TaskHandle::new(RUNTIME.spawn(async move {
            listener.on_update(vec![VectorDiff::Reset { values: initial_values }.map(Into::into)]);
            
            while let Some(diffs) = stream.next().await {
                listener.on_update(diffs.into_iter().map(|diff| diff.map(Into::into)).collect());
            }
        }))
    }
}

#[derive(uniffi::Record)]
pub struct RoomDirectorySearchEntriesResult {
    pub entries_stream: Arc<TaskHandle>,
}

#[derive(uniffi::Enum)]
pub enum RoomDirectorySearchEntryUpdate {
    Clear,
    Append { values: Vec<RoomDescription> },
}

#[uniffi::export(callback_interface)]
pub trait RoomDirectorySearchEntriesListener: Send + Sync + Debug {
    fn on_update(&self, room_entries_update: RoomDirectorySearchEntryUpdate);
}
