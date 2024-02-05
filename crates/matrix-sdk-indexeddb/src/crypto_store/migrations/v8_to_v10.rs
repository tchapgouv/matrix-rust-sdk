// Copyright 2024 The Matrix.org Foundation C.I.C.
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

//! Migration code that moves from inbound_group_sessions2 to
//! inbound_group_sessions3, shrinking the values stored in each record.

use indexed_db_futures::{
    idb_object_store::IdbObjectStore, IdbDatabase, IdbIndex, IdbKeyPath, IdbQuerySource,
};
use matrix_sdk_crypto::olm::InboundGroupSession;
use tracing::{debug, info};
use web_sys::{DomException, IdbIndexParameters, IdbTransactionMode};

use crate::{
    crypto_store::{
        indexeddb_serializer::IndexeddbSerializer,
        keys,
        migrations::{do_schema_upgrade, old_keys, v7::InboundGroupSessionIndexedDbObject2},
        InboundGroupSessionIndexedDbObject, Result,
    },
    IndexeddbCryptoStoreError,
};

fn add_nonunique_index<'a>(
    object_store: &'a IdbObjectStore<'a>,
    name: &str,
    key_path: &str,
) -> Result<IdbIndex<'a>, DomException> {
    let mut params = IdbIndexParameters::new();
    params.unique(false);
    object_store.create_index_with_params(name, &IdbKeyPath::str(key_path), &params)
}

/// Perform the schema upgrade v8 to v9, creating `inbound_group_sessions3`.
pub(crate) async fn upgrade_scheme_to_v9_create_inbound_group_sessions3(
    name: &str,
) -> Result<(), DomException> {
    do_schema_upgrade(name, 9, |db, _| {
        let object_store = db.create_object_store(keys::INBOUND_GROUP_SESSIONS_V3)?;

        add_nonunique_index(
            &object_store,
            keys::INBOUND_GROUP_SESSIONS_BACKUP_INDEX,
            "needs_backup",
        )?;

        // See https://github.com/element-hq/element-web/issues/26892#issuecomment-1906336076
        // for the plan concerning this property and index. At time of writing, it is
        // unused, and needs_backup is still used.
        add_nonunique_index(
            &object_store,
            keys::INBOUND_GROUP_SESSIONS_BACKED_UP_TO_INDEX,
            "backed_up_to",
        )?;

        Ok(())
    })
    .await
}

/// Migrate data from `inbound_group_sessions2` into `inbound_group_sessions3`.
pub(crate) async fn migrate_data_before_v10_populate_inbound_group_sessions3(
    name: &str,
    serializer: &IndexeddbSerializer,
) -> Result<()> {
    info!("IndexeddbCryptoStore migrate data before v10 starting");

    let db = IdbDatabase::open(name)?.await?;
    let txn = db.transaction_on_multi_with_mode(
        &[old_keys::INBOUND_GROUP_SESSIONS_V2, keys::INBOUND_GROUP_SESSIONS_V3],
        IdbTransactionMode::Readwrite,
    )?;

    let inbound_group_sessions2 = txn.object_store(old_keys::INBOUND_GROUP_SESSIONS_V2)?;
    let inbound_group_sessions3 = txn.object_store(keys::INBOUND_GROUP_SESSIONS_V3)?;

    let row_count = inbound_group_sessions2.count()?.await?;
    info!(row_count, "Shrinking inbound_group_session records");

    // Iterate through all rows
    if let Some(cursor) = inbound_group_sessions2.open_cursor()?.await? {
        let mut idx = 0;
        loop {
            idx += 1;

            if idx % 100 == 0 {
                debug!("Migrating session {idx} of {row_count}");
            }

            // Deserialize the session from the old store
            let old_value: InboundGroupSessionIndexedDbObject2 =
                serde_wasm_bindgen::from_value(cursor.value())?;

            let session = InboundGroupSession::from_pickle(
                serializer.deserialize_value_from_bytes(&old_value.pickled_session)?,
            )
            .map_err(|e| IndexeddbCryptoStoreError::CryptoStoreError(e.into()))?;

            // Calculate its key in the new table
            let new_key = serializer.encode_key(
                keys::INBOUND_GROUP_SESSIONS_V3,
                (&session.room_id, session.session_id()),
            );

            // Serialize the session in the new format
            // This is much the same as [`IndexeddbStore::serialize_inbound_group_session`].
            let new_value = InboundGroupSessionIndexedDbObject::new(
                serializer.maybe_encrypt_value(session.pickle().await)?,
                !session.backed_up(),
            );

            // Write it to the new store
            inbound_group_sessions3
                .add_key_val(&new_key, &serde_wasm_bindgen::to_value(&new_value)?)?;

            // We are done with the original data, so delete it now.
            cursor.delete()?;

            // Continue to the next record, or stop if we're done
            if !cursor.continue_cursor()?.await? {
                debug!("Migrated {idx} sessions.");
                break;
            }
        }
    }

    // We have finished with the old store. Clear it, since it is faster to
    // clear+delete than just delete. See https://www.artificialworlds.net/blog/2024/02/01/deleting-an-indexed-db-store-can-be-incredibly-slow-on-firefox/
    // for more details.
    inbound_group_sessions2.clear()?.await?;

    txn.await.into_result()?;
    db.close();
    info!("IndexeddbCryptoStore upgrade data before v10 finished");

    Ok(())
}

/// Perform the schema upgrade v8 to v10, deleting `inbound_group_sessions2`.
pub(crate) async fn upgrade_scheme_to_v10_delete_inbound_group_sessions2(
    name: &str,
) -> Result<(), DomException> {
    do_schema_upgrade(name, 10, |db, _| {
        db.delete_object_store(old_keys::INBOUND_GROUP_SESSIONS_V2)?;
        Ok(())
    })
    .await
}
