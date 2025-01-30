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

/// The State that is indicated by the BWI Content Scanner
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BWIScanState {
    /// The Content is marked as safe
    Trusted,

    /// The content is marked as infected and must not be loaded
    Infected,

    /**
    The content can not be scanned.
    That could happen because the ContentScanner is not available
    or the content can not be uploaded.
    */
    Error,

    /// The scan process is triggered bug not finished
    InProgress,

    /// The file can no longer be found and can therefore not be scanned
    NotFound,
}
