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

use std::collections::BTreeMap;
use std::convert::TryInto;

use olm_rs::sas::OlmSas;

use matrix_sdk_common::api::r0::keys::{AlgorithmAndDeviceId, KeyAlgorithm};
use matrix_sdk_common::events::{key::verification::mac::MacEventContent, ToDeviceEvent};
use matrix_sdk_common::identifiers::DeviceId;

use crate::{Account, Device};

#[allow(dead_code)]
mod sas;

struct SasIds {
    account: Account,
    other_device: Device,
}

fn get_emoji(index: u8) -> (&'static str, &'static str) {
    match index {
        0 => ("🐶", "Dog"),
        1 => ("🐱", "Cat"),
        2 => ("🦁", "Lion"),
        3 => ("🐎", "Horse"),
        4 => ("🦄", "Unicorn"),
        5 => ("🐷", "Pig"),
        6 => ("🐘", "Elephant"),
        7 => ("🐰", "Rabbit"),
        8 => ("🐼", "Panda"),
        9 => ("🐓", "Rooster"),
        10 => ("🐧", "Penguin"),
        11 => ("🐢", "Turtle"),
        12 => ("🐟", "Fish"),
        13 => ("🐙", "Octopus"),
        14 => ("🦋", "Butterfly"),
        15 => ("🌷", "Flower"),
        16 => ("🌳", "Tree"),
        17 => ("🌵", "Cactus"),
        18 => ("🍄", "Mushroom"),
        19 => ("🌏", "Globe"),
        20 => ("🌙", "Moon"),
        21 => ("☁️", "Cloud"),
        22 => ("🔥", "Fire"),
        23 => ("🍌", "Banana"),
        24 => ("🍎", "Apple"),
        25 => ("🍓", "Strawberry"),
        26 => ("🌽", "Corn"),
        27 => ("🍕", "Pizza"),
        28 => ("🎂", "Cake"),
        29 => ("❤️", "Heart"),
        30 => ("😀", "Smiley"),
        31 => ("🤖", "Robot"),
        32 => ("🎩", "Hat"),
        33 => ("👓", "Glasses"),
        34 => ("🔧", "Spanner"),
        35 => ("🎅", "Santa"),
        36 => ("👍", "Thumbs up"),
        37 => ("☂️", "Umbrella"),
        38 => ("⌛", "Hourglass"),
        39 => ("⏰", "Clock"),
        40 => ("🎁", "Gift"),
        41 => ("💡", "Light Bulb"),
        42 => ("📕", "Book"),
        43 => ("✏️", "Pencil"),
        44 => ("📎", "Paperclip"),
        45 => ("✂️", "Scissors"),
        46 => ("🔒", "Lock"),
        47 => ("🔑", "Key"),
        48 => ("🔨", "Hammer"),
        49 => ("☎️", "Telephone"),
        50 => ("🏁", "Flag"),
        51 => ("🚂", "Train"),
        52 => ("🚲", "Bicycle"),
        53 => ("✈️", "Airplane"),
        54 => ("🚀", "Rocket"),
        55 => ("🏆", "Trophy"),
        56 => ("⚽", "Ball"),
        57 => ("🎸", "Guitar"),
        58 => ("🎺", "Trumpet"),
        59 => ("🔔", "Bell"),
        60 => ("⚓", "Anchor"),
        61 => ("🎧", "Headphones"),
        62 => ("📁", "Folder"),
        63 => ("📌", "Pin"),
        _ => panic!("Trying to fetch an SAS emoji outside the allowed range"),
    }
}

fn extra_mac_info_receive(ids: &SasIds, flow_id: &str) -> String {
    format!(
        "MATRIX_KEY_VERIFICATION_MAC{first_user}{first_device}\
        {second_user}{second_device}{transaction_id}",
        first_user = ids.other_device.user_id(),
        first_device = ids.other_device.device_id(),
        second_user = ids.account.user_id(),
        second_device = ids.account.device_id(),
        transaction_id = flow_id,
    )
}

fn receive_mac_event(
    sas: &OlmSas,
    ids: &SasIds,
    flow_id: &str,
    event: &ToDeviceEvent<MacEventContent>,
) -> (Vec<Box<DeviceId>>, Vec<String>) {
    let mut verified_devices: Vec<Box<DeviceId>> = Vec::new();

    let info = extra_mac_info_receive(&ids, flow_id);

    let mut keys = event.content.mac.keys().cloned().collect::<Vec<String>>();
    keys.sort();
    let keys = sas
        .calculate_mac(&keys.join(","), &format!("{}KEYIDS", &info))
        .expect("Can't calculate SAS MAC");

    if keys != event.content.keys {
        panic!("Keys mac mismatch")
    }

    for (key_id, key_mac) in &event.content.mac {
        let split: Vec<&str> = key_id.splitn(2, ":").collect();

        if split.len() != 2 {
            continue;
        }

        let algorithm: KeyAlgorithm = if let Ok(a) = split[0].try_into() {
            a
        } else {
            continue;
        };

        let id = split[1];
        let device_key_id = AlgorithmAndDeviceId(algorithm, id.into());

        if let Some(key) = ids.other_device.keys().get(&device_key_id) {
            if key_mac
                == &sas
                    .calculate_mac(key, &format!("{}{}", info, key_id))
                    .expect("Can't calculate SAS MAC")
            {
                verified_devices.push(ids.other_device.device_id().into());
            }
        }
        // TODO add an else branch for the master key here
    }

    (verified_devices, vec![])
}

fn extra_mac_info_send(ids: &SasIds, flow_id: &str) -> String {
    format!(
        "MATRIX_KEY_VERIFICATION_MAC{first_user}{first_device}\
        {second_user}{second_device}{transaction_id}",
        first_user = ids.account.user_id(),
        first_device = ids.account.device_id(),
        second_user = ids.other_device.user_id(),
        second_device = ids.other_device.device_id(),
        transaction_id = flow_id,
    )
}

fn get_mac_content(sas: &OlmSas, ids: &SasIds, flow_id: &str) -> MacEventContent {
    let mut mac: BTreeMap<String, String> = BTreeMap::new();

    let key_id = AlgorithmAndDeviceId(KeyAlgorithm::Ed25519, ids.account.device_id().into());
    let key = ids.account.identity_keys().ed25519();
    let info = extra_mac_info_send(ids, flow_id);

    mac.insert(
        key_id.to_string(),
        sas.calculate_mac(key, &format!("{}{}", info, key_id))
            .expect("Can't calculate SAS MAC"),
    );

    // TODO Add the cross signing master key here if we trust/have it.

    let mut keys = mac.keys().cloned().collect::<Vec<String>>();
    keys.sort();
    let keys = sas
        .calculate_mac(&keys.join(","), &format!("{}KEYIDS", &info))
        .expect("Can't calculate SAS MAC");

    MacEventContent {
        transaction_id: flow_id.to_owned(),
        keys,
        mac,
    }
}
