/*
 * Copyright (c) 2025 BWI GmbH
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
use ruma_common::UserId;

mod test;

pub struct BWIFederationHandler {
    server_domain: String,
}

impl BWIFederationHandler {
    pub fn for_server(server_url: &str) -> Self {
        BWIFederationHandler { server_domain: server_url.to_owned() }
    }

    pub fn for_user_id(user_id: &UserId) -> Self {
        let server = user_id.server_name().host();
        Self::for_server(server)
    }

    fn server_domain(&self) -> &str {
        &self.server_domain
    }

    pub fn create_server_acl(&self, is_federated: bool) -> Vec<String> {
        match is_federated {
            // Room is federated, allow other user from other servers to join the room
            true => vec!["*".to_owned()],
            // Room is not federated, only user from the same homeserver can join the room
            false => vec![self.server_domain().to_owned()],
        }
    }
}
