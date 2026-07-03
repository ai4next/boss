use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use boss_api::ResourceVersion;
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;

use crate::error::{StoreError, StoreResult};
use crate::storage::{Storage, WatchEvent, WatchStream, extract_rv, set_rv};
use crate::watch::WatchBus;

/// In-memory storage backend. A `BTreeMap<key, (value, rv)>` ordered by key
/// makes prefix scans trivial. A monotonic global counter allocates resource
/// versions. Suitable for single-node dev and tests.
pub struct MemoryStorage {
    inner: RwLock<MemoryInner>,
    bus: WatchBus,
}

#[derive(Default)]
struct MemoryInner {
    revision: u64,
    data: BTreeMap<String, (serde_json::Value, ResourceVersion)>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(MemoryInner::default()),
            bus: WatchBus::new(1024),
        }
    }

    pub fn arc() -> Arc<Self> {
        Arc::new(Self::new())
    }

    fn next_rv(inner: &mut MemoryInner) -> ResourceVersion {
        inner.revision += 1;
        ResourceVersion(inner.revision)
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn create<T>(&self, key: &str, value: &T) -> StoreResult<T>
    where
        T: Serialize + DeserializeOwned + Send + Sync,
    {
        let mut json = serde_json::to_value(value)?;
        let (rv, event) = {
            let mut inner = self.inner.write();
            if inner.data.contains_key(key) {
                return Err(StoreError::AlreadyExists(key.to_string()));
            }
            let rv = Self::next_rv(&mut inner);
            set_rv(&mut json, rv);
            inner.data.insert(key.to_string(), (json.clone(), rv));
            (rv, WatchEvent::Added(key.to_string(), json.clone()))
        };
        self.bus.publish(key.to_string(), event);
        tracing::debug!(%key, rv = rv.0, "created");
        Ok(serde_json::from_value(json)?)
    }

    async fn get<T>(&self, key: &str) -> StoreResult<T>
    where
        T: DeserializeOwned + Send + Sync,
    {
        let inner = self.inner.read();
        match inner.data.get(key) {
            Some((value, _rv)) => Ok(serde_json::from_value(value.clone())?),
            None => Err(StoreError::NotFound(key.to_string())),
        }
    }

    async fn update<T>(&self, key: &str, value: &T) -> StoreResult<T>
    where
        T: Serialize + DeserializeOwned + Send + Sync,
    {
        let mut json = serde_json::to_value(value)?;
        let incoming_rv = extract_rv(&json);
        let (rv, event) = {
            let mut inner = self.inner.write();
            let stored_rv = match inner.data.get(key) {
                Some((_v, rv)) => *rv,
                None => return Err(StoreError::NotFound(key.to_string())),
            };
            // CAS: incoming rv must match stored rv.
            if incoming_rv != Some(stored_rv) {
                return Err(StoreError::Conflict(format!(
                    "resourceVersion mismatch for {key}: expected {stored_rv:?}, got {incoming_rv:?}"
                )));
            }
            let rv = Self::next_rv(&mut inner);
            set_rv(&mut json, rv);
            inner.data.insert(key.to_string(), (json.clone(), rv));
            (rv, WatchEvent::Modified(key.to_string(), json.clone()))
        };
        self.bus.publish(key.to_string(), event);
        tracing::debug!(%key, rv = rv.0, "updated");
        Ok(serde_json::from_value(json)?)
    }

    async fn delete(&self, key: &str) -> StoreResult<()> {
        let removed = {
            let mut inner = self.inner.write();
            match inner.data.remove(key) {
                Some((mut value, _rv)) => {
                    let rv = Self::next_rv(&mut inner);
                    set_rv(&mut value, rv);
                    Some(value)
                }
                None => return Err(StoreError::NotFound(key.to_string())),
            }
        };
        if let Some(value) = removed {
            self.bus
                .publish(key.to_string(), WatchEvent::Deleted(key.to_string(), value));
        }
        tracing::debug!(%key, "deleted");
        Ok(())
    }

    async fn list<T>(&self, prefix: &str) -> StoreResult<Vec<T>>
    where
        T: Serialize + DeserializeOwned + Send + Sync,
    {
        let inner = self.inner.read();
        let mut out = Vec::new();
        for (key, (value, _rv)) in inner.data.iter() {
            if key.starts_with(prefix) {
                out.push(serde_json::from_value(value.clone())?);
            }
        }
        Ok(out)
    }

    async fn watch(&self, prefix: &str, start_rv: ResourceVersion) -> StoreResult<WatchStream> {
        Ok(self.bus.subscribe(prefix.to_string(), start_rv))
    }

    async fn current_revision(&self) -> StoreResult<ResourceVersion> {
        Ok(ResourceVersion(self.inner.read().revision))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boss_api::{Object, PodSpec, Resource};
    use tokio_stream::StreamExt;

    fn pod(name: &str) -> Object<PodSpec> {
        Object::new(name, "default", PodSpec::default())
    }

    #[tokio::test]
    async fn delete_event_advances_resource_version() {
        let storage = MemoryStorage::new();
        let key = "/registry/pods/default/demo";
        let created: Object<PodSpec> = storage.create(key, &pod("demo")).await.unwrap();
        let created_rv = created.metadata.resource_version;
        let mut watch = storage.watch("/registry/pods/", created_rv).await.unwrap();

        storage.delete(key).await.unwrap();

        let event = watch.next().await.unwrap();
        let deleted_rv = extract_rv(event.object()).unwrap();
        assert!(deleted_rv > created_rv);
    }

    #[test]
    fn object_new_uses_pod_kind() {
        let pod = pod("demo");
        assert_eq!(pod.type_meta.kind, PodSpec::KIND);
    }
}
