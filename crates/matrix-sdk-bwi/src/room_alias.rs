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

use regex::Regex;

#[derive(Debug)]
pub struct BWIRoomAlias {}

impl BWIRoomAlias {
    pub fn alias_for_room_name(room_name: &str) -> String {
        let re = Regex::new(r"[^a-z0-9]").unwrap();
        let lowercase_room_name = room_name.to_lowercase();
        let alias = re.replace_all(&lowercase_room_name, "");
        let milliseconds_timestamp: u128 =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        alias.into_owned() + &*milliseconds_timestamp.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alias_for_empty_string() {
        let room_name = "";
        let alias = BWIRoomAlias::alias_for_room_name(&room_name.to_string());
        assert!(alias.parse::<u128>().is_ok());
        assert!(alias.len() > 10);
    }

    #[test]
    fn alias_for_lowercase_string() {
        let room_name = "abc123def";
        let alias = BWIRoomAlias::alias_for_room_name(&room_name.to_string());
        assert!(alias.starts_with(room_name));
    }

    #[test]
    fn alias_for_uppercase_string() {
        let room_name = "AbC123dEf";
        let alias = BWIRoomAlias::alias_for_room_name(&room_name.to_string());
        let room_name_lowercase = room_name.to_lowercase();
        assert!(alias.starts_with(room_name_lowercase.as_str()));
    }

    #[test]
    fn alias_for_room_name_with_special_chars() {
        let room_name = "AbC123dEf";
        let special_chars = "Öü$";
        let room_name_with_special_chars = special_chars.to_owned() + room_name;
        let alias = BWIRoomAlias::alias_for_room_name(&room_name_with_special_chars.to_string());
        let room_name_lowercase = room_name.to_lowercase();
        assert!(alias.starts_with(room_name_lowercase.as_str()));
    }
}
