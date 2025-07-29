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
#![cfg(test)]
mod pipeline_tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_mock() {
        // Start a background HTTP server on a random local port
        let mock_server = MockServer::start().await;

        // Arrange the behaviour of the MockServer adding a Mock:
        // when it receives a GET request on '/hello' it will respond with a 200.
        Mock::given(method("GET"))
            .and(path("/hello"))
            .respond_with(ResponseTemplate::new(200))
            // Mounting the mock on the mock server - it's now effective!
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::builder().build().unwrap();

        // If we probe the MockServer using any HTTP client it behaves as expected.
        let status =
            client.get(format!("{}/hello", &mock_server.uri())).send().await.unwrap().status();
        assert_eq!(status, 200);

        // If the request doesn't match any `Mock` mounted on our `MockServer` a 404 is returned.
        let status =
            client.get(format!("{}/missing", &mock_server.uri())).send().await.unwrap().status();
        assert_eq!(status, 404);
    }
}
