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

pub struct BWIImprint {
    imprint_url: String,
}

pub trait BWIImprintSource: Sync + Send {
    fn get_imprint(&self) -> BWIImprint;
}

impl BWIImprint {
    pub fn new(imprint_url: &str) -> Self {
        Self { imprint_url: String::from(imprint_url) }
    }

    pub fn as_url(&self) -> &str {
        &self.imprint_url
    }
}
