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
mod test;
use url::Url;

pub struct BWIFederationHandler {
    server_url: Url,
}

impl BWIFederationHandler {
    pub fn for_server(server_url: Url) -> Self {
        BWIFederationHandler { server_url }
    }

    fn server_domain(&self) -> String {
        self.server_url.domain().expect("The url of the domain should be valid").to_owned()
    }

    pub fn create_server_acl(&self, is_federated: bool) -> Vec<String> {
        let mut is_allowed: Vec<String> = Vec::new();
        if is_federated {
            // Room is federated, allow other user from other servers to join the room
            is_allowed.push("*".to_owned());
        } else {
            // Room is not federated, only user from the same homeserver can join the room
            is_allowed.push(self.server_domain());
        }
        is_allowed
    }
}
