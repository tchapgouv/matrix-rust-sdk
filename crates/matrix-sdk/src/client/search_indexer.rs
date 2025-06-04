use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use matrix_sdk_base::deserialized_responses::TimelineEvent;
use ruma::{serde::JsonObject, UserId};
use seshat::{Database, Event, Profile, SearchConfig};
use thiserror::Error;
use tracing::info;

use crate::Room;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    SeshatError(seshat::Error),
    #[error(transparent)]
    SerdeError(serde_json::Error),
    #[error("Missing field: {0}")]
    MissingField(String),
}

#[derive(Clone)]
pub struct SearchIndexer {
    database: Arc<RwLock<Database>>,
    index_unencrypted_events: bool,
}

impl SearchIndexer {
    pub fn new(db_path: PathBuf, index_unencrypted_events: bool) -> Result<Self, Error> {
        let database = Database::new(db_path).map_err(Error::SeshatError)?;
        Ok(Self { database: Arc::new(RwLock::new(database)), index_unencrypted_events })
    }

    pub async fn add_live_event(&self, room: &Room, event: &TimelineEvent) -> Result<(), Error> {
        if !self.index_unencrypted_events {
            if event.encryption_info().is_none() {
                return Ok(());
            }
        }

        let raw_event = event.raw();
        if let Some(event_type) = raw_event.get_field("type").unwrap_or_default() {
            let sender_str: String =
                raw_event.get_field("sender").unwrap_or_default().unwrap_or_default();
            let sender = room
                .get_member_no_sync(&UserId::parse(sender_str).unwrap())
                .await
                .unwrap_or_default();
            let displayname =
                sender.as_ref().map(|member| member.display_name().map(String::from)).flatten();
            let avatar_url = sender
                .as_ref()
                .map(|member| member.avatar_url().map(|mxc| mxc.to_string()))
                .flatten();

            let profile = Profile { displayname, avatar_url };

            let content: JsonObject =
                raw_event.get_field("content").unwrap_or_default().unwrap_or_default();

            let seshat_event = Event::new(
                event_type,
                serde_json::to_string(&content).unwrap_or_default().as_str(),
                raw_event.get_field("msgtype").unwrap_or_default().unwrap_or_default(),
                raw_event.get_field("event_id").unwrap_or_default().unwrap_or_default(),
                raw_event.get_field("sender").unwrap_or_default().unwrap_or_default(),
                raw_event.get_field("origin_server_ts").unwrap_or_default().unwrap_or_default(),
                room.room_id().as_str(),
                serde_json::to_string(&raw_event).unwrap_or_default().as_str(),
            );
            info!("Adding event {} to search index", seshat_event.event_id);
            self.database.read().unwrap().add_event(seshat_event, profile);
        }
        Ok(())
    }

    pub fn commit(&self) -> Result<(), Error> {
        info!("Committing search index");
        self.database.write().unwrap().commit().map_err(Error::SeshatError)?;
        Ok(())
    }

    pub fn search(
        &self,
        query: &str,
        limit: u32,
        room_id: Option<&str>,
    ) -> Result<Vec<String>, Error> {
        let mut search_config = SearchConfig::default();
        search_config.limit(limit as usize);
        if let Some(room_id) = room_id {
            search_config.for_room(room_id);
        }
        let search_batch = self
            .database
            .read()
            .unwrap()
            .search(query, &search_config)
            .map_err(Error::SeshatError)?;
        let mut results = Vec::new();
        for result in search_batch.results.iter() {
            let event_source = serde_json::from_str::<JsonObject>(result.event_source.as_str())
                .map_err(Error::SerdeError)?;
            let event_id = event_source
                .get("event_id")
                .ok_or(Error::MissingField("event_id".to_string()))?
                .to_string();
            results.push(event_id);
        }
        Ok(results)
    }
}
