use serde::{Deserialize, Serialize};

use crate::{Condition, Resource, pod::PodSpec, selector::LabelSelector};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodTemplateSpec {
    #[serde(default)]
    pub metadata: crate::ObjectMeta,
    #[serde(default)]
    pub spec: PodSpec,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentStrategyType {
    #[default]
    Recreate,
    RollingUpdate,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentStrategy {
    #[serde(default)]
    #[serde(rename = "type")]
    pub kind: DeploymentStrategyType,
    #[serde(
        default,
        rename = "rollingUpdate",
        skip_serializing_if = "Option::is_none"
    )]
    pub rolling_update: Option<RollingUpdateDeployment>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollingUpdateDeployment {
    #[serde(
        default,
        rename = "maxUnavailable",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_unavailable: Option<String>,
    #[serde(default, rename = "maxSurge", skip_serializing_if = "Option::is_none")]
    pub max_surge: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentSpec {
    #[serde(default)]
    pub replicas: i32,
    pub selector: LabelSelector,
    pub template: PodTemplateSpec,
    #[serde(default)]
    pub strategy: DeploymentStrategy,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentStatus {
    #[serde(default, rename = "observedGeneration")]
    pub observed_generation: i64,
    #[serde(default)]
    pub replicas: i32,
    #[serde(default, rename = "readyReplicas")]
    pub ready_replicas: i32,
    #[serde(default, rename = "updatedReplicas")]
    pub updated_replicas: i32,
    #[serde(default, rename = "availableReplicas")]
    pub available_replicas: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,
}

pub type Deployment = crate::Object<DeploymentSpec>;

impl Resource for DeploymentSpec {
    type Status = DeploymentStatus;
    const KIND: &'static str = "Deployment";
    const API_VERSION: &'static str = "boss.io/apps/v1";
}
