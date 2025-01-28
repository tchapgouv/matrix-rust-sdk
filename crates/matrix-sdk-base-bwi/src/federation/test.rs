/*
 * Copyright (c) 2024 BWI GmbH
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

mod test_federation_create_server_acl {
    use crate::federation::BWIFederationHandler;
    use url::Url;

    const SERVER_URL: &str = "https://test.de/";

    #[test]
    fn test_is_federated() {
        let server_url = Url::parse(SERVER_URL).unwrap();

        // Act
        let federation_handler = BWIFederationHandler::for_server(server_url);

        let allow = federation_handler.create_server_acl(true);

        // Assert
        assert!(allow.contains(&"*".to_string()));
        assert_eq!(allow.iter().count(), 1);
    }

    #[test]
    fn test_is_not_federated() {
        let server_url = Url::parse(SERVER_URL).unwrap();

        // Act
        let federation_handler = BWIFederationHandler::for_server(server_url);

        let allow = federation_handler.create_server_acl(false);

        // Assert
        assert!(allow.contains(&"test.de".to_string()));
        assert_eq!(allow.iter().count(), 1);
    }
}
