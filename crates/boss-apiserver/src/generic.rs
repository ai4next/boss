//! Generic CRUD over `T: Resource`, shared by every resource handler. The
//! type discriminator, uid, creation timestamp and generation are defaulted on
//! create; optimistic concurrency is enforced in the storage layer via
//! `metadata.resourceVersion`.

use boss_api::{Object, ObjectList, Resource, ResourceVersion, TypeMeta};
use boss_store::{Storage, build_key, build_prefix};

use crate::error::ApiResult;
use crate::state::AppState;

pub async fn create<T: Resource>(
    state: &AppState,
    resource: &str,
    namespace: Option<&str>,
    mut obj: Object<T>,
) -> ApiResult<Object<T>> {
    // Defaulting. For namespaced resources, fill namespace from the URL if
    // absent; cluster-scoped resources (namespace == None) keep it empty.
    if let Some(ns) = namespace
        && !ns.is_empty()
        && obj.metadata.namespace.is_empty()
    {
        obj.metadata.namespace = ns.to_string();
    }
    if obj.metadata.uid.is_none() {
        obj.metadata.uid = Some(boss_common::id::new_uid());
    }
    if obj.metadata.creation_timestamp.is_none() {
        obj.metadata.creation_timestamp = Some(boss_common::time::now_rfc3339());
    }
    obj.metadata.generation = 1;
    obj.type_meta = TypeMeta {
        api_version: T::API_VERSION.to_string(),
        kind: T::KIND.to_string(),
    };

    let key = build_key(resource, Some(&obj.metadata.namespace), &obj.metadata.name);
    let created = state.storage.create(&key, &obj).await?;
    Ok(created)
}

pub async fn get<T: Resource>(
    state: &AppState,
    resource: &str,
    namespace: Option<&str>,
    name: &str,
) -> ApiResult<Object<T>> {
    let key = build_key(resource, namespace, name);
    let obj: Object<T> = state.storage.get(&key).await?;
    Ok(obj)
}

pub async fn list<T: Resource>(
    state: &AppState,
    resource: &str,
    namespace: Option<&str>,
) -> ApiResult<ObjectList<T>> {
    let prefix = build_prefix(resource, namespace);
    let items: Vec<Object<T>> = state.storage.list(&prefix).await?;
    let rv = state
        .storage
        .current_revision()
        .await
        .unwrap_or(ResourceVersion(0));
    Ok(ObjectList::new(items, rv))
}

/// Full update (or status subresource update): the client must send the
/// `metadata.resourceVersion` it read; the storage layer CAS-checks it.
pub async fn update<T: Resource>(
    state: &AppState,
    resource: &str,
    namespace: Option<&str>,
    name: &str,
    mut body: Object<T>,
) -> ApiResult<Object<T>> {
    // Force name/namespace consistency with the URL.
    body.metadata.name = name.to_string();
    if let Some(ns) = namespace
        && !ns.is_empty()
    {
        body.metadata.namespace = ns.to_string();
    }
    body.type_meta = TypeMeta {
        api_version: T::API_VERSION.to_string(),
        kind: T::KIND.to_string(),
    };
    let key = build_key(resource, namespace, name);
    let updated = state.storage.update::<Object<T>>(&key, &body).await?;
    Ok(updated)
}

pub async fn delete<T: Resource>(
    state: &AppState,
    resource: &str,
    namespace: Option<&str>,
    name: &str,
) -> ApiResult<()> {
    let key = build_key(resource, namespace, name);
    state.storage.delete(&key).await?;
    Ok(())
}
