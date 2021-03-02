#[cfg(target_os = "linux")]
mod perf;

use std::convert::TryFrom;

use criterion::{async_executor::FuturesExecutor, *};

use futures::executor::block_on;
use matrix_sdk_common::{
    api::r0::{
        keys::{claim_keys, get_keys},
        to_device::send_event_to_device::Response as ToDeviceResponse,
    },
    identifiers::{room_id, user_id, DeviceIdBox, UserId},
    uuid::Uuid,
};
use matrix_sdk_crypto::{EncryptionSettings, OlmMachine};
use matrix_sdk_test::response_from_file;
use serde_json::Value;

fn alice_id() -> UserId {
    user_id!("@alice:example.org")
}

fn alice_device_id() -> DeviceIdBox {
    "JLAFKJWSCS".into()
}

fn keys_query_response() -> get_keys::Response {
    let data = include_bytes!("./keys_query.json");
    let data: Value = serde_json::from_slice(data).unwrap();
    let data = response_from_file(&data);
    get_keys::Response::try_from(data).expect("Can't parse the keys upload response")
}

fn keys_claim_response() -> claim_keys::Response {
    let data = include_bytes!("./keys_claim.json");
    let data: Value = serde_json::from_slice(data).unwrap();
    let data = response_from_file(&data);
    claim_keys::Response::try_from(data).expect("Can't parse the keys upload response")
}

pub fn keys_query(c: &mut Criterion) {
    let machine = OlmMachine::new(&alice_id(), &alice_device_id());
    let response = keys_query_response();
    let uuid = Uuid::new_v4();

    let count = response
        .device_keys
        .values()
        .fold(0, |acc, d| acc + d.len())
        + response.master_keys.len()
        + response.self_signing_keys.len()
        + response.user_signing_keys.len();

    let mut group = c.benchmark_group("Keys querying");
    group.throughput(Throughput::Elements(count as u64));

    let name = format!("{} device and cross signing keys", count);

    group.bench_with_input(
        BenchmarkId::new("memory store", &name),
        &response,
        |b, response| {
            b.to_async(FuturesExecutor)
                .iter(|| async { machine.mark_request_as_sent(&uuid, response).await.unwrap() })
        },
    );

    let dir = tempfile::tempdir().unwrap();
    let machine = block_on(OlmMachine::new_with_default_store(
        &alice_id(),
        &alice_device_id(),
        dir.path(),
        None,
    ))
    .unwrap();

    group.bench_with_input(
        BenchmarkId::new("sled store", &name),
        &response,
        |b, response| {
            b.to_async(FuturesExecutor)
                .iter(|| async { machine.mark_request_as_sent(&uuid, response).await.unwrap() })
        },
    );

    group.finish()
}

pub fn keys_claiming(c: &mut Criterion) {
    let keys_query_response = keys_query_response();
    let uuid = Uuid::new_v4();

    let response = keys_claim_response();

    let count = response
        .one_time_keys
        .values()
        .fold(0, |acc, d| acc + d.len());

    let mut group = c.benchmark_group("Olm session creation");
    group.throughput(Throughput::Elements(count as u64));

    let name = format!("{} one-time keys", count);

    group.bench_with_input(
        BenchmarkId::new("memory store", &name),
        &response,
        |b, response| {
            b.iter_batched(
                || {
                    let machine = OlmMachine::new(&alice_id(), &alice_device_id());
                    block_on(machine.mark_request_as_sent(&uuid, &keys_query_response)).unwrap();
                    machine
                },
                move |machine| block_on(machine.mark_request_as_sent(&uuid, response)).unwrap(),
                BatchSize::SmallInput,
            )
        },
    );

    group.bench_with_input(
        BenchmarkId::new("sled store", &name),
        &response,
        |b, response| {
            b.iter_batched(
                || {
                    let dir = tempfile::tempdir().unwrap();
                    let machine = block_on(OlmMachine::new_with_default_store(
                        &alice_id(),
                        &alice_device_id(),
                        dir.path(),
                        None,
                    ))
                    .unwrap();
                    block_on(machine.mark_request_as_sent(&uuid, &keys_query_response)).unwrap();
                    machine
                },
                move |machine| block_on(machine.mark_request_as_sent(&uuid, response)).unwrap(),
                BatchSize::SmallInput,
            )
        },
    );

    group.finish()
}

pub fn room_key_sharing(c: &mut Criterion) {
    let keys_query_response = keys_query_response();
    let uuid = Uuid::new_v4();
    let response = keys_claim_response();
    let room_id = room_id!("!test:localhost");

    let to_device_response = ToDeviceResponse::new();
    let users: Vec<UserId> = keys_query_response.device_keys.keys().cloned().collect();

    let count = response
        .one_time_keys
        .values()
        .fold(0, |acc, d| acc + d.len());

    let machine = OlmMachine::new(&alice_id(), &alice_device_id());
    block_on(machine.mark_request_as_sent(&uuid, &keys_query_response)).unwrap();
    block_on(machine.mark_request_as_sent(&uuid, &response)).unwrap();

    let mut group = c.benchmark_group("Room key sharing");
    group.throughput(Throughput::Elements(count as u64));
    let name = format!("{} devices", count);

    group.bench_function(BenchmarkId::new("memory store", &name), |b| {
        b.to_async(FuturesExecutor).iter(|| async {
            let requests = machine
                .share_group_session(&room_id, users.iter(), EncryptionSettings::default())
                .await
                .unwrap();

            assert!(requests.len() >= 8);

            for request in requests {
                machine
                    .mark_request_as_sent(&request.txn_id, &to_device_response)
                    .await
                    .unwrap();
            }

            machine.invalidate_group_session(&room_id);
        })
    });

    let dir = tempfile::tempdir().unwrap();
    let machine = block_on(OlmMachine::new_with_default_store(
        &alice_id(),
        &alice_device_id(),
        dir.path(),
        None,
    ))
    .unwrap();
    block_on(machine.mark_request_as_sent(&uuid, &keys_query_response)).unwrap();
    block_on(machine.mark_request_as_sent(&uuid, &response)).unwrap();

    group.bench_function(BenchmarkId::new("sled store", &name), |b| {
        b.to_async(FuturesExecutor).iter(|| async {
            let requests = machine
                .share_group_session(&room_id, users.iter(), EncryptionSettings::default())
                .await
                .unwrap();

            assert!(requests.len() >= 8);

            for request in requests {
                machine
                    .mark_request_as_sent(&request.txn_id, &to_device_response)
                    .await
                    .unwrap();
            }

            machine.invalidate_group_session(&room_id);
        })
    });

    group.finish()
}

fn criterion() -> Criterion {
    #[cfg(target_os = "linux")]
    let criterion = Criterion::default().with_profiler(perf::FlamegraphProfiler::new(100));
    #[cfg(not(target_os = "linux"))]
    let criterion = Criterion::default();

    criterion
}

criterion_group! {
    name = benches;
    config = criterion();
    targets = keys_query, keys_claiming, room_key_sharing
}
criterion_main!(benches);
