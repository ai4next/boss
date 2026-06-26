use serde::{Deserialize, Serialize};

use crate::{Condition, Resource, deployment::PodTemplateSpec, selector::LabelSelector};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSetSpec {
    #[serde(default)]
    pub replicas: i32,
    pub selector: LabelSelector,
    pub template: PodTemplateSpec,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSetStatus {
    #[serde(default, rename = "observedGeneration")]
    pub observed_generation: i64,
    #[serde(default)]
    pub replicas: i32,
    #[serde(default, rename = "readyReplicas")]
    pub ready_replicas: i32,
    #[serde(default, rename = "availableReplicas")]
    pub available_replicas: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,
}

pub type ReplicaSet = crate::Object<ReplicaSetSpec>;

impl Resource for ReplicaSetSpec {
    type Status = ReplicaSetStatus;
    const KIND: &'static str = "ReplicaSet";
    const API_VERSION: &'static str = "boss.io/apps/v1";
}
