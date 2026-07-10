use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{ObjectMeta, ResourceVersion, TypeMeta};

/// Trait implemented by every resource's spec type, binding spec ↔ status and
/// providing the type discriminator constants.
pub trait Resource:
    Clone + std::fmt::Debug + Send + Sync + Serialize + DeserializeOwned + 'static
{
    type Status: Clone
        + std::fmt::Debug
        + Send
        + Sync
        + Serialize
        + DeserializeOwned
        + Default
        + 'static;
    const KIND: &'static str;
    const API_VERSION: &'static str;
}

/// Generic Boss object wrapper: `type_meta + metadata + spec + status`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "T: serde::Serialize, T::Status: serde::Serialize",
    deserialize = "T: serde::de::DeserializeOwned, T::Status: serde::de::DeserializeOwned"
))]
#[serde(rename_all = "camelCase")]
pub struct Object<T: Resource> {
    #[serde(flatten)]
    pub type_meta: TypeMeta,
    pub metadata: ObjectMeta,
    pub spec: T,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<T::Status>,
}

impl<T: Resource> Object<T> {
    pub fn new(name: impl Into<String>, namespace: impl Into<String>, spec: T) -> Self {
        Self {
            type_meta: TypeMeta {
                api_version: T::API_VERSION.to_string(),
                kind: T::KIND.to_string(),
            },
            metadata: ObjectMeta {
                name: name.into(),
                namespace: namespace.into(),
                ..Default::default()
            },
            spec,
            status: None,
        }
    }

    pub fn with_status(mut self, status: T::Status) -> Self {
        self.status = Some(status);
        self
    }

    pub fn default_metadata(&mut self, namespace: Option<&str>) {
        if let Some(ns) = namespace
            && !ns.is_empty()
            && self.metadata.namespace.is_empty()
        {
            self.metadata.namespace = ns.to_string();
        }
        if self.metadata.uid.is_none() {
            self.metadata.uid = Some(boss_common::id::new_uid());
        }
        if self.metadata.creation_timestamp.is_none() {
            self.metadata.creation_timestamp = Some(boss_common::time::now_rfc3339());
        }
        self.metadata.generation = 1;
        self.type_meta = TypeMeta {
            api_version: T::API_VERSION.to_string(),
            kind: T::KIND.to_string(),
        };
    }
}

/// List response wrapper, carrying the list's resource version.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "T: serde::Serialize",
    deserialize = "T: serde::de::DeserializeOwned"
))]
#[serde(rename_all = "camelCase")]
pub struct ObjectList<T: Resource> {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: ListMeta,
    pub items: Vec<Object<T>>,
}

impl<T: Resource> ObjectList<T> {
    pub fn new(items: Vec<Object<T>>, rv: ResourceVersion) -> Self {
        Self {
            api_version: T::API_VERSION.to_string(),
            kind: format!("{}List", T::KIND),
            metadata: ListMeta {
                resource_version: rv,
            },
            items,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListMeta {
    #[serde(rename = "resourceVersion")]
    pub resource_version: ResourceVersion,
}
