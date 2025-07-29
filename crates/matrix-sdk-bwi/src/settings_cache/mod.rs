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
use async_trait::async_trait;
use matrix_sdk_base::store::DynStateStore;
use matrix_sdk_base::StoreError;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::marker::PhantomData;
use tracing::warn;

pub trait Storable: Serialize + DeserializeOwned + Send + Sync {}

impl<T> Storable for T where T: Serialize + DeserializeOwned + Send + Sync {}

pub struct BWISetting<'a, T: Storable> {
    key: &'a str,
    phantom: PhantomData<T>,
}

impl<'a, T: Storable> BWISetting<'a, T> {
    pub(crate) const fn new(key: &'a str) -> Self {
        Self { key, phantom: PhantomData }
    }

    fn get_key(&self) -> &str {
        self.key
    }
}

#[async_trait]
pub trait BWISettingsCache {
    async fn try_load<T: Storable>(&self, setting: &BWISetting<'_, T>) -> Option<T>;

    async fn store<T: Storable>(
        &self,
        setting: &BWISetting<'_, T>,
        value: T,
    ) -> Result<(), StoreError>;
}

#[async_trait]
impl BWISettingsCache for DynStateStore {
    async fn try_load<T: Storable>(&self, setting: &BWISetting<'_, T>) -> Option<T> {
        let loaded_value = self.get_custom_value(setting.get_key().as_bytes()).await;
        match loaded_value {
            Ok(Some(raw)) => serde_json::from_slice::<T>(&raw)
                .inspect_err(|err| warn!("###BWI### Loading of value failed: {}", err))
                .ok(),
            Ok(None) | Err(_) => None,
        }
    }

    async fn store<T: Storable>(
        &self,
        setting: &BWISetting<'_, T>,
        value: T,
    ) -> Result<(), StoreError> {
        self.set_custom_value(setting.get_key().as_bytes(), serde_json::to_vec(&value)?)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod test {
    use super::BWISettingsCache;
    use crate::settings_cache::BWISetting;
    use matrix_sdk_base::store::{IntoStateStore, MemoryStore};

    #[tokio::test]
    async fn test_load_and_store_for_in_memory_store() {
        // Arrange
        const EXAMPLE_TEXT: &str = "Hello World";

        let example_setting = BWISetting::<String>::new("foo");
        let store = MemoryStore::default().into_state_store();

        // Act
        store.store(&example_setting, EXAMPLE_TEXT.to_owned()).await.unwrap();
        let loaded = store.try_load(&example_setting).await;

        // Assert
        assert_eq!(loaded, Some(EXAMPLE_TEXT.to_owned()));
    }

    #[tokio::test]
    async fn test_not_cached_for_in_memory_store() {
        // Arrange
        let example_setting = BWISetting::<String>::new("foo");
        let store = MemoryStore::default().into_state_store();

        // Act
        let loaded = store.try_load(&example_setting).await;

        // Assert
        assert_eq!(loaded, None);
    }
}
