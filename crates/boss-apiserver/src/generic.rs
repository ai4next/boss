//! Generic CRUD over `T: Resource`, shared by every resource handler. The
//! type discriminator, uid, creation timestamp and generation are defaulted on
//! create; optimistic concurrency is enforced in the storage layer via
//! `metadata.resourceVersion`.

use boss_api::{Object, ObjectList, Resource, ResourceVersion, TypeMeta};
use boss_store::{Storage, build_key, build_prefix};

use crate::error::{ApiError, ApiResult};
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

/// Full spec/metadata update. The client must send the `metadata.resourceVersion`
/// it read; the storage layer CAS-checks it. Status is preserved and can only be
/// changed through the status subresource.
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
    let current: Object<T> = state.storage.get(&key).await?;
    if body.metadata.uid.is_some() && current.metadata.uid != body.metadata.uid {
        return Err(ApiError::Invalid(
            "metadata.uid is immutable and must match the current object".to_string(),
        ));
    }
    if serde_json::to_value(&current.spec).map_err(boss_common::BossError::from)?
        != serde_json::to_value(&body.spec).map_err(boss_common::BossError::from)?
    {
        body.metadata.generation = current.metadata.generation + 1;
    } else {
        body.metadata.generation = current.metadata.generation;
    }
    body.metadata.creation_timestamp = current.metadata.creation_timestamp;
    body.metadata.uid = current.metadata.uid;
    body.status = current.status;
    let updated = state.storage.update::<Object<T>>(&key, &body).await?;
    Ok(updated)
}

/// Status subresource update. Only the status field from the request body is
/// applied; spec and metadata stay aligned with the current stored object.
pub async fn update_status<T: Resource>(
    state: &AppState,
    resource: &str,
    namespace: Option<&str>,
    name: &str,
    body: Object<T>,
) -> ApiResult<Object<T>> {
    let key = build_key(resource, namespace, name);
    let mut current: Object<T> = state.storage.get(&key).await?;
    if body.metadata.resource_version != current.metadata.resource_version {
        return Err(boss_common::BossError::Conflict(format!(
            "resourceVersion mismatch for {key}: expected {:?}, got {:?}",
            current.metadata.resource_version, body.metadata.resource_version
        ))
        .into());
    }
    current.status = body.status;
    let updated = state.storage.update::<Object<T>>(&key, &current).await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use boss_api::{Container, Object, PodPhase, PodSpec, PodStatus};
    use boss_store::{MemoryStorage, StorageBackend};
    use std::sync::Arc;

    fn state() -> AppState {
        AppState::new(Arc::new(StorageBackend::Memory(MemoryStorage::arc())))
    }

    fn pod(name: &str) -> Object<PodSpec> {
        Object::new(
            name,
            "default",
            PodSpec {
                containers: vec![Container {
                    name: "main".to_string(),
                    command: vec!["sleep".to_string()],
                    args: vec!["1".to_string()],
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
    }

    #[tokio::test]
    async fn normal_update_preserves_status_and_bumps_generation_on_spec_change() {
        let state = state();
        let created = create(&state, "pods", Some("default"), pod("demo"))
            .await
            .unwrap();
        let mut status_body = created.clone();
        status_body.status = Some(PodStatus {
            phase: PodPhase::Running,
            ..Default::default()
        });
        let with_status = update_status(&state, "pods", Some("default"), "demo", status_body)
            .await
            .unwrap();

        let mut spec_update = with_status.clone();
        spec_update.spec.containers[0].args = vec!["2".to_string()];
        spec_update.status = None;
        let updated = update(&state, "pods", Some("default"), "demo", spec_update)
            .await
            .unwrap();

        assert_eq!(updated.metadata.generation, 2);
        assert_eq!(updated.status.unwrap().phase, PodPhase::Running);
    }

    #[tokio::test]
    async fn status_update_preserves_spec() {
        let state = state();
        let created = create(&state, "pods", Some("default"), pod("demo"))
            .await
            .unwrap();

        let mut body = created.clone();
        body.spec.containers.clear();
        body.status = Some(PodStatus {
            phase: PodPhase::Failed,
            ..Default::default()
        });
        let updated = update_status(&state, "pods", Some("default"), "demo", body)
            .await
            .unwrap();

        assert_eq!(updated.spec.containers.len(), 1);
        assert_eq!(updated.status.unwrap().phase, PodPhase::Failed);
    }
}
