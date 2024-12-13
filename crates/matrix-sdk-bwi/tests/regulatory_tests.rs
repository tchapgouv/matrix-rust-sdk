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

use matrix_sdk_bwi::regulatory::organization::BWIOrganization;

const TEST_URL: &str = "example.com";

#[tokio::test]
async fn test_regulatory_from_well_known_file() {
    let organization = BWIOrganization::from_homeserver_url(TEST_URL).await.unwrap();

    let imprint_url = organization.get_imprint();
    let privacy_policy_url = organization.get_data_privacy();

    assert_eq!(imprint_url.as_url(), "https://www.bwi.de/impressum");
    assert_eq!(privacy_policy_url.as_url(), "https://messenger.bwi.de/datenschutz-bundesmessenger");
}
