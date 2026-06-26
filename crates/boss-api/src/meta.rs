use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::ResourceVersion;

/// `apiVersion` + `kind`, the type discriminator on every object.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeMeta {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
}

pub type Labels = BTreeMap<String, String>;
pub type Annotations = BTreeMap<String, String>;

/// Reference from an object to its owner (for cascading ownership, e.g. RS→Pod).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerReference {
    pub api_version: String,
    pub kind: String,
    pub name: String,
    pub uid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<bool>,
}

/// Standard resource condition used by controllers to explain convergence.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(
        default,
        rename = "observedGeneration",
        skip_serializing_if = "Option::is_none"
    )]
    pub observed_generation: Option<i64>,
    #[serde(
        default,
        rename = "lastTransitionTime",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_transition_time: Option<String>,
}

impl Condition {
    pub fn new(
        kind: impl Into<String>,
        status: impl Into<String>,
        reason: impl Into<String>,
        message: impl Into<String>,
        observed_generation: i64,
    ) -> Self {
        Self {
            kind: kind.into(),
            status: status.into(),
            reason: Some(reason.into()),
            message: Some(message.into()),
            observed_generation: Some(observed_generation),
            last_transition_time: Some(boss_common::time::now_rfc3339()),
        }
    }
}

/// Standard metadata shared by every resource object.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectMeta {
    pub name: String,
    #[serde(default)]
    pub namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(default)]
    pub resource_version: ResourceVersion,
    #[serde(default)]
    pub generation: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Labels>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owner_references: Vec<OwnerReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creation_timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletion_timestamp: Option<String>,
}

impl ObjectMeta {
    pub fn key(&self, resource: &str) -> (String, String, String) {
        (
            resource.to_string(),
            self.namespace.clone(),
            self.name.clone(),
        )
    }
}
