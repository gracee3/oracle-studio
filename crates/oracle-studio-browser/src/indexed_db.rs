use std::rc::Rc;

use rexie::{ObjectStore, Rexie, TransactionMode};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::WorkerGlobalScope;

use super::*;

const DATABASE: &str = "oracle-studio";
const VERSION: u32 = 1;
const VAULTS: &str = "encrypted_vaults";
const CATALOGS: &str = "catalog_objects";
const SETTINGS: &str = "settings";
const ACTIVE_CATALOG: &str = "active_catalog";

#[derive(Clone)]
pub struct IndexedDbStore {
    db: Rc<Rexie>,
}

#[derive(Serialize, Deserialize)]
struct CatalogRecord {
    input: CatalogInstallInput,
    metadata: CatalogMetadata,
}

impl IndexedDbStore {
    pub async fn open() -> Result<Self, StoreError> {
        let db = Rexie::builder(DATABASE)
            .version(VERSION)
            .add_object_store(ObjectStore::new(VAULTS))
            .add_object_store(ObjectStore::new(CATALOGS))
            .add_object_store(ObjectStore::new(SETTINGS))
            .build()
            .await
            .map_err(browser)?;
        Ok(Self { db: Rc::new(db) })
    }
}

impl BrowserStore for IndexedDbStore {
    fn list_vaults(&self) -> StoreFuture<'_, Vec<VaultRecord>> {
        Box::pin(async move {
            let transaction = self
                .db
                .transaction(&[VAULTS], TransactionMode::ReadOnly)
                .map_err(browser)?;
            let store = transaction.store(VAULTS).map_err(browser)?;
            let values = store.get_all(None, None).await.map_err(browser)?;
            let records = values
                .into_iter()
                .map(from_json)
                .collect::<Result<Vec<_>, _>>()?;
            transaction.done().await.map_err(browser)?;
            Ok(records)
        })
    }

    fn insert_vault(&self, record: VaultRecord) -> StoreFuture<'_, ()> {
        Box::pin(async move {
            let transaction = self
                .db
                .transaction(&[VAULTS], TransactionMode::ReadWrite)
                .map_err(browser)?;
            let store = transaction.store(VAULTS).map_err(browser)?;
            let key = JsValue::from_str(&record.id);
            if store.key_exists(key.clone()).await.map_err(browser)? {
                transaction.abort().await.map_err(browser)?;
                return Err(StoreError::Duplicate);
            }
            store
                .add(&to_json(&record)?, Some(&key))
                .await
                .map_err(browser)?;
            transaction.done().await.map_err(browser)?;
            Ok(())
        })
    }

    fn replace_vault(&self, record: VaultRecord, expected: String) -> StoreFuture<'_, ()> {
        Box::pin(async move {
            let transaction = self
                .db
                .transaction(&[VAULTS], TransactionMode::ReadWrite)
                .map_err(browser)?;
            let store = transaction.store(VAULTS).map_err(browser)?;
            let key = JsValue::from_str(&record.id);
            let Some(value) = store.get(key.clone()).await.map_err(browser)? else {
                transaction.abort().await.map_err(browser)?;
                return Err(StoreError::NotFound);
            };
            let existing: VaultRecord = from_json(value)?;
            if existing.revision != expected {
                transaction.abort().await.map_err(browser)?;
                return Err(StoreError::Conflict);
            }
            store
                .put(&to_json(&record)?, Some(&key))
                .await
                .map_err(browser)?;
            transaction.done().await.map_err(browser)?;
            Ok(())
        })
    }

    fn delete_vault(&self, id: String) -> StoreFuture<'_, ()> {
        Box::pin(async move {
            let transaction = self
                .db
                .transaction(&[VAULTS], TransactionMode::ReadWrite)
                .map_err(browser)?;
            let store = transaction.store(VAULTS).map_err(browser)?;
            let key = JsValue::from_str(&id);
            if !store.key_exists(key.clone()).await.map_err(browser)? {
                transaction.abort().await.map_err(browser)?;
                return Err(StoreError::NotFound);
            }
            store.delete(key).await.map_err(browser)?;
            transaction.done().await.map_err(browser)?;
            Ok(())
        })
    }

    fn load_catalog(&self) -> StoreFuture<'_, Option<CatalogInstallInput>> {
        Box::pin(async move {
            let transaction = self
                .db
                .transaction(&[CATALOGS, SETTINGS], TransactionMode::ReadOnly)
                .map_err(browser)?;
            let settings = transaction.store(SETTINGS).map_err(browser)?;
            let Some(active) = settings
                .get(JsValue::from_str(ACTIVE_CATALOG))
                .await
                .map_err(browser)?
            else {
                transaction.done().await.map_err(browser)?;
                return Ok(None);
            };
            let content_id = active
                .as_string()
                .ok_or_else(|| StoreError::Corrupt("active catalog pointer is not text".into()))?;
            let catalogs = transaction.store(CATALOGS).map_err(browser)?;
            let value = catalogs
                .get(JsValue::from_str(&content_id))
                .await
                .map_err(browser)?
                .ok_or_else(|| StoreError::Corrupt("active catalog object is missing".into()))?;
            let record: CatalogRecord = from_json(value)?;
            if record.metadata.content_id != content_id {
                return Err(StoreError::Corrupt(
                    "active catalog content ID mismatch".into(),
                ));
            }
            transaction.done().await.map_err(browser)?;
            Ok(Some(record.input))
        })
    }

    fn save_catalog(
        &self,
        input: CatalogInstallInput,
        metadata: CatalogMetadata,
    ) -> StoreFuture<'_, ()> {
        Box::pin(async move {
            let transaction = self
                .db
                .transaction(&[CATALOGS, SETTINGS], TransactionMode::ReadWrite)
                .map_err(browser)?;
            let content_id = metadata.content_id.clone();
            let catalogs = transaction.store(CATALOGS).map_err(browser)?;
            let record = CatalogRecord { input, metadata };
            let key = JsValue::from_str(&content_id);
            catalogs
                .put(&to_json(&record)?, Some(&key))
                .await
                .map_err(browser)?;
            let settings = transaction.store(SETTINGS).map_err(browser)?;
            settings
                .put(
                    &JsValue::from_str(&content_id),
                    Some(&JsValue::from_str(ACTIVE_CATALOG)),
                )
                .await
                .map_err(browser)?;
            transaction.done().await.map_err(browser)?;
            Ok(())
        })
    }

    fn request_persistence(&self) -> StoreFuture<'_, bool> {
        Box::pin(async move {
            let global: WorkerGlobalScope = js_sys::global().unchecked_into();
            let promise = global.navigator().storage().persist().map_err(js_error)?;
            JsFuture::from(promise)
                .await
                .map_err(js_error)?
                .as_bool()
                .ok_or_else(|| {
                    StoreError::Browser("storage persistence returned a non-boolean value".into())
                })
        })
    }
}

fn to_json(value: &impl Serialize) -> Result<JsValue, StoreError> {
    serde_json::to_string(value)
        .map(|value| JsValue::from_str(&value))
        .map_err(|error| StoreError::Corrupt(error.to_string()))
}

fn from_json<T: for<'de> Deserialize<'de>>(value: JsValue) -> Result<T, StoreError> {
    let value = value
        .as_string()
        .ok_or_else(|| StoreError::Corrupt("IndexedDB record is not JSON text".into()))?;
    serde_json::from_str(&value).map_err(|error| StoreError::Corrupt(error.to_string()))
}

fn browser(error: rexie::Error) -> StoreError {
    StoreError::Browser(error.to_string())
}
fn js_error(error: JsValue) -> StoreError {
    StoreError::Browser(format!("{error:?}"))
}
