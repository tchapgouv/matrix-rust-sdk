// Copyright 2020 The Matrix.org Foundation C.I.C.
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

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::{DashMap, ReadOnlyView};
use tokio::sync::Mutex;

use super::device::Device;
use super::olm::{InboundGroupSession, Session};

#[derive(Debug)]
pub struct SessionStore {
    entries: HashMap<String, Arc<Mutex<Vec<Arc<Mutex<Session>>>>>>,
}

impl SessionStore {
    pub fn new() -> Self {
        SessionStore {
            entries: HashMap::new(),
        }
    }

    pub async fn add(&mut self, session: Session) -> Arc<Mutex<Session>> {
        if !self.entries.contains_key(&session.sender_key) {
            self.entries.insert(
                session.sender_key.to_owned(),
                Arc::new(Mutex::new(Vec::new())),
            );
        }
        let sessions = self.entries.get_mut(&session.sender_key).unwrap();
        let session = Arc::new(Mutex::new(session));
        sessions.lock().await.push(session.clone());

        session
    }

    pub fn get(&self, sender_key: &str) -> Option<Arc<Mutex<Vec<Arc<Mutex<Session>>>>>> {
        self.entries.get(sender_key).cloned()
    }

    pub fn set_for_sender(&mut self, sender_key: &str, sessions: Vec<Arc<Mutex<Session>>>) {
        self.entries
            .insert(sender_key.to_owned(), Arc::new(Mutex::new(sessions)));
    }
}

#[derive(Debug)]
pub struct GroupSessionStore {
    entries: HashMap<String, HashMap<String, HashMap<String, Arc<Mutex<InboundGroupSession>>>>>,
}

impl GroupSessionStore {
    pub fn new() -> Self {
        GroupSessionStore {
            entries: HashMap::new(),
        }
    }

    pub fn add(&mut self, session: InboundGroupSession) -> bool {
        if !self.entries.contains_key(&session.room_id) {
            self.entries
                .insert(session.room_id.to_owned(), HashMap::new());
        }

        let room_map = self.entries.get_mut(&session.room_id).unwrap();

        if !room_map.contains_key(&session.sender_key) {
            room_map.insert(session.sender_key.to_owned(), HashMap::new());
        }

        let sender_map = room_map.get_mut(&session.sender_key).unwrap();
        let ret = sender_map.insert(session.session_id(), Arc::new(Mutex::new(session)));

        ret.is_some()
    }

    pub fn get(
        &self,
        room_id: &str,
        sender_key: &str,
        session_id: &str,
    ) -> Option<Arc<Mutex<InboundGroupSession>>> {
        self.entries
            .get(room_id)
            .and_then(|m| m.get(sender_key).and_then(|m| m.get(session_id).cloned()))
    }
}

#[derive(Clone, Debug)]
pub struct DeviceStore {
    entries: Arc<DashMap<String, DashMap<String, Device>>>,
}

pub struct UserDevices {
    entries: ReadOnlyView<String, Device>,
}

impl UserDevices {
    pub fn get(&self, device_id: &str) -> Option<Device> {
        self.entries.get(device_id).cloned()
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    pub fn devices(&self) -> impl Iterator<Item = &Device> {
        self.entries.values()
    }
}

impl DeviceStore {
    pub fn new() -> Self {
        DeviceStore {
            entries: Arc::new(DashMap::new()),
        }
    }

    pub fn add(&self, device: Device) -> bool {
        if !self.entries.contains_key(device.user_id()) {
            self.entries
                .insert(device.user_id().to_owned(), DashMap::new());
        }
        let device_map = self.entries.get_mut(device.user_id()).unwrap();

        device_map
            .insert(device.device_id().to_owned(), device)
            .is_some()
    }

    pub fn get(&self, user_id: &str, device_id: &str) -> Option<Device> {
        self.entries
            .get(user_id)
            .and_then(|m| m.get(device_id).map(|d| d.value().clone()))
    }

    pub fn user_devices(&self, user_id: &str) -> UserDevices {
        if !self.entries.contains_key(user_id) {
            self.entries.insert(user_id.to_owned(), DashMap::new());
        }
        UserDevices {
            entries: self.entries.get(user_id).unwrap().clone().into_read_only(),
        }
    }
}
