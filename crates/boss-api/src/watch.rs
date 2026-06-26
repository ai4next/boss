use serde::{Deserialize, Serialize};

use crate::ResourceVersion;

/// Event type on a watch stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EventType {
    Added,
    Modified,
    Deleted,
}

/// A single watch event carrying the resource version and the object payload
/// (as raw JSON to stay generic across resource kinds).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchEvent {
    #[serde(rename = "type")]
    pub kind: EventType,
    #[serde(rename = "resourceVersion")]
    pub resource_version: ResourceVersion,
    /// The full object serialized as a JSON value.
    pub object: serde_json::Value,
}

impl WatchEvent {
    pub fn added(rv: ResourceVersion, object: serde_json::Value) -> Self {
        Self {
            kind: EventType::Added,
            resource_version: rv,
            object,
        }
    }
    pub fn modified(rv: ResourceVersion, object: serde_json::Value) -> Self {
        Self {
            kind: EventType::Modified,
            resource_version: rv,
            object,
        }
    }
    pub fn deleted(rv: ResourceVersion, object: serde_json::Value) -> Self {
        Self {
            kind: EventType::Deleted,
            resource_version: rv,
            object,
        }
    }
}

/// Parameters for a list/watch request.
#[derive(Clone, Debug, Default)]
pub struct ListParams {
    pub watch: bool,
    pub resource_version: Option<ResourceVersion>,
}
