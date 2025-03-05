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

#[cfg(test)]
mod test_federation_create_server_acl {
    use crate::federation::BWIFederationHandler;
    use ruma_common::OwnedUserId;
    use std::str::FromStr;

    const SERVER_DOMAIN: &str = "test.de";

    #[test]
    fn test_is_federated() {
        // Act
        let federation_handler = BWIFederationHandler::for_server(SERVER_DOMAIN);

        let allow = federation_handler.create_server_acl(true);

        // Assert
        assert!(allow.contains(&"*".to_string()));
        assert_eq!(allow.iter().count(), 1);
    }

    #[test]
    fn test_is_not_federated() {
        // Act
        let federation_handler = BWIFederationHandler::for_server(SERVER_DOMAIN);

        let allow = federation_handler.create_server_acl(false);

        // Assert
        assert_eq!(allow, vec!["test.de".to_string()]);
    }

    #[test]
    fn test_is_not_federated_from_user_id() {
        // Arrange
        let user_id = OwnedUserId::from_str("@example.user:test.de").unwrap();

        // Act
        let federation_handler = BWIFederationHandler::for_user_id(&user_id);

        let allow = federation_handler.create_server_acl(false);

        // Assert
        assert_eq!(allow, vec!["test.de".to_string()]);
    }
}
