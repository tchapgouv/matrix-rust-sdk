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
use wiremock::{Match, Request};

pub struct NotMatcher<T: Match> {
    inner_matcher: T,
}

impl<S: Match> Match for NotMatcher<S> {
    fn matches(&self, request: &Request) -> bool {
        let is_inner_matching = self.inner_matcher.matches(request);
        !is_inner_matching
    }
}

pub fn not<T: Match>(inner_matcher: T) -> NotMatcher<T> {
    NotMatcher { inner_matcher }
}
