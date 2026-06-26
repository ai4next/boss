use std::pin::Pin;

use async_trait::async_trait;
use boss_api::ResourceVersion;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use tokio_stream::Stream;

use crate::error::{StoreError, StoreResult};

/// A single watch event. The payload is the full object as a JSON value; for
/// `Deleted` it is the previous object (still carries its resourceVersion).
#[derive(Clone, Debug)]
pub enum WatchEvent {
    Added(String, serde_json::Value),
    Modified(String, serde_json::Value),
    Deleted(String, serde_json::Value),
}

impl WatchEvent {
    pub fn key(&self) -> &str {
        match self {
            WatchEvent::Added(k, _) | WatchEvent::Modified(k, _) | WatchEvent::Deleted(k, _) => k,
        }
    }
    pub fn object(&self) -> &serde_json::Value {
        match self {
            WatchEvent::Added(_, v) | WatchEvent::Modified(_, v) | WatchEvent::Deleted(_, v) => v,
        }
    }
}

/// Boxed, sendable stream of watch events.
pub type WatchStream = Pin<Box<dyn Stream<Item = WatchEvent> + Send>>;

/// Consistent storage abstraction. All methods are generic over the value type
/// and (de)serialize at the boundary. Optimistic concurrency is enforced via
/// `metadata.resourceVersion`: `create` requires the key to be absent; `update`
/// requires the incoming `resourceVersion` to match the stored one.
#[async_trait]
pub trait Storage: Send + Sync {
    async fn create<T>(&self, key: &str, value: &T) -> StoreResult<T>
    where
        T: Serialize + DeserializeOwned + Send + Sync;

    async fn get<T>(&self, key: &str) -> StoreResult<T>
    where
        T: DeserializeOwned + Send + Sync;

    async fn update<T>(&self, key: &str, value: &T) -> StoreResult<T>
    where
        T: Serialize + DeserializeOwned + Send + Sync;

    async fn delete(&self, key: &str) -> StoreResult<()>;

    async fn list<T>(&self, prefix: &str) -> StoreResult<Vec<T>>
    where
        T: Serialize + DeserializeOwned + Send + Sync;

    async fn watch(&self, prefix: &str, start_rv: ResourceVersion) -> StoreResult<WatchStream>;

    async fn current_revision(&self) -> StoreResult<ResourceVersion>;
}

/// Runtime-selected storage backend. Components hold `Arc<StorageBackend>` and
/// call through the `Storage` impl. Raft joins here in Phase 5.
#[derive(Clone)]
pub enum StorageBackend {
    Memory(std::sync::Arc<crate::MemoryStorage>),
}

#[async_trait]
impl Storage for StorageBackend {
    async fn create<T>(&self, key: &str, value: &T) -> StoreResult<T>
    where
        T: Serialize + DeserializeOwned + Send + Sync,
    {
        match self {
            StorageBackend::Memory(m) => m.create(key, value).await,
        }
    }

    async fn get<T>(&self, key: &str) -> StoreResult<T>
    where
        T: DeserializeOwned + Send + Sync,
    {
        match self {
            StorageBackend::Memory(m) => m.get(key).await,
        }
    }

    async fn update<T>(&self, key: &str, value: &T) -> StoreResult<T>
    where
        T: Serialize + DeserializeOwned + Send + Sync,
    {
        match self {
            StorageBackend::Memory(m) => m.update(key, value).await,
        }
    }

    async fn delete(&self, key: &str) -> StoreResult<()> {
        match self {
            StorageBackend::Memory(m) => m.delete(key).await,
        }
    }

    async fn list<T>(&self, prefix: &str) -> StoreResult<Vec<T>>
    where
        T: Serialize + DeserializeOwned + Send + Sync,
    {
        match self {
            StorageBackend::Memory(m) => m.list(prefix).await,
        }
    }

    async fn watch(&self, prefix: &str, start_rv: ResourceVersion) -> StoreResult<WatchStream> {
        match self {
            StorageBackend::Memory(m) => m.watch(prefix, start_rv).await,
        }
    }

    async fn current_revision(&self) -> StoreResult<ResourceVersion> {
        match self {
            StorageBackend::Memory(m) => m.current_revision().await,
        }
    }
}

// Re-export the blanket impl so `Arc<StorageBackend>` is also `Storage`.
#[async_trait]
impl<S: Storage + ?Sized> Storage for std::sync::Arc<S> {
    async fn create<T>(&self, key: &str, value: &T) -> StoreResult<T>
    where
        T: Serialize + DeserializeOwned + Send + Sync,
    {
        (**self).create(key, value).await
    }
    async fn get<T>(&self, key: &str) -> StoreResult<T>
    where
        T: DeserializeOwned + Send + Sync,
    {
        (**self).get(key).await
    }
    async fn update<T>(&self, key: &str, value: &T) -> StoreResult<T>
    where
        T: Serialize + DeserializeOwned + Send + Sync,
    {
        (**self).update(key, value).await
    }
    async fn delete(&self, key: &str) -> StoreResult<()> {
        (**self).delete(key).await
    }
    async fn list<T>(&self, prefix: &str) -> StoreResult<Vec<T>>
    where
        T: Serialize + DeserializeOwned + Send + Sync,
    {
        (**self).list(prefix).await
    }
    async fn watch(&self, prefix: &str, start_rv: ResourceVersion) -> StoreResult<WatchStream> {
        (**self).watch(prefix, start_rv).await
    }
    async fn current_revision(&self) -> StoreResult<ResourceVersion> {
        (**self).current_revision().await
    }
}

/// Helper: read `metadata.resourceVersion` (as u64) from a JSON value.
pub(crate) fn extract_rv(value: &serde_json::Value) -> Option<ResourceVersion> {
    value
        .get("metadata")
        .and_then(|m| m.get("resourceVersion"))
        .and_then(|v| v.as_u64())
        .map(ResourceVersion)
}

/// Helper: set `metadata.resourceVersion` on a JSON value (mutating).
pub(crate) fn set_rv(value: &mut serde_json::Value, rv: ResourceVersion) {
    if let Some(meta) = value.get_mut("metadata").and_then(|m| m.as_object_mut()) {
        meta.insert("resourceVersion".to_string(), serde_json::json!(rv.0));
    }
}

/// Map a storage error to the public StoreError. (Currently a passthrough.)
#[allow(unused)]
fn map_err(e: StoreError) -> StoreError {
    e
}
