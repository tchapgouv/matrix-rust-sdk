// Copyright 2020 The Matrix.org Foundation C.I.C.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[allow(dead_code)]
mod machine;
#[allow(dead_code)]
mod sas;

pub use machine::VerificationMachine;
pub use sas::Sas;

#[cfg(test)]
pub(crate) mod test {
    use serde_json::Value;

    use matrix_sdk_common::{
        api::r0::to_device::send_event_to_device::Request as ToDeviceRequest,
        events::{AnyToDeviceEvent, AnyToDeviceEventContent, EventType, ToDeviceEvent},
        identifiers::UserId,
    };

    pub(crate) fn wrap_any_to_device_content(
        sender: &UserId,
        content: AnyToDeviceEventContent,
    ) -> AnyToDeviceEvent {
        match content {
            AnyToDeviceEventContent::KeyVerificationKey(c) => {
                AnyToDeviceEvent::KeyVerificationKey(ToDeviceEvent {
                    sender: sender.clone(),
                    content: c,
                })
            }
            AnyToDeviceEventContent::KeyVerificationStart(c) => {
                AnyToDeviceEvent::KeyVerificationStart(ToDeviceEvent {
                    sender: sender.clone(),
                    content: c,
                })
            }
            AnyToDeviceEventContent::KeyVerificationAccept(c) => {
                AnyToDeviceEvent::KeyVerificationAccept(ToDeviceEvent {
                    sender: sender.clone(),
                    content: c,
                })
            }
            AnyToDeviceEventContent::KeyVerificationMac(c) => {
                AnyToDeviceEvent::KeyVerificationMac(ToDeviceEvent {
                    sender: sender.clone(),
                    content: c,
                })
            }

            _ => unreachable!(),
        }
    }

    pub(crate) fn get_content_from_request(request: &ToDeviceRequest) -> AnyToDeviceEventContent {
        let json: Value = serde_json::from_str(
            request
                .messages
                .values()
                .next()
                .unwrap()
                .values()
                .next()
                .unwrap()
                .get(),
        )
        .unwrap();

        match request.event_type {
            EventType::KeyVerificationStart => {
                AnyToDeviceEventContent::KeyVerificationStart(serde_json::from_value(json).unwrap())
            }
            EventType::KeyVerificationKey => {
                AnyToDeviceEventContent::KeyVerificationKey(serde_json::from_value(json).unwrap())
            }
            EventType::KeyVerificationAccept => AnyToDeviceEventContent::KeyVerificationAccept(
                serde_json::from_value(json).unwrap(),
            ),
            EventType::KeyVerificationMac => {
                AnyToDeviceEventContent::KeyVerificationMac(serde_json::from_value(json).unwrap())
            }
            EventType::KeyVerificationCancel => AnyToDeviceEventContent::KeyVerificationCancel(
                serde_json::from_value(json).unwrap(),
            ),
            _ => unreachable!(),
        }
    }
}
