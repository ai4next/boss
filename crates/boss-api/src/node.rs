use serde::{Deserialize, Serialize};

use crate::{Resource, ResourceList, meta::Labels};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Taint {
    pub key: String,
    pub value: String,
    pub effect: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSpec {
    #[serde(default, rename = "podCIDR", skip_serializing_if = "Option::is_none")]
    pub pod_cidr: Option<String>,
    #[serde(default)]
    pub taints: Vec<Taint>,
    #[serde(default)]
    pub unschedulable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Labels>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeCondition {
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
    #[serde(
        default,
        rename = "lastHeartbeatTime",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_heartbeat_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeAddress {
    #[serde(rename = "type")]
    pub kind: String,
    pub address: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSystemInfo {
    #[serde(
        default,
        rename = "kernelVersion",
        skip_serializing_if = "Option::is_none"
    )]
    pub kernel_version: Option<String>,
    #[serde(default, rename = "osImage", skip_serializing_if = "Option::is_none")]
    pub os_image: Option<String>,
    #[serde(
        default,
        rename = "architecture",
        skip_serializing_if = "Option::is_none"
    )]
    pub architecture: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilities {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<RuntimeProviderStatus>,
}

impl RuntimeCapabilities {
    pub fn healthy_provider_for_class(&self, class: &str) -> Option<&RuntimeProviderStatus> {
        self.providers
            .iter()
            .find(|provider| provider.healthy && provider.supports_class(class))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProviderStatus {
    pub name: String,
    #[serde(default)]
    pub healthy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    #[serde(
        default,
        rename = "artifactTypes",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub artifact_types: Vec<String>,
    #[serde(
        default,
        rename = "networkModes",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub network_modes: Vec<String>,
    #[serde(
        default,
        rename = "isolationLevels",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub isolation_levels: Vec<String>,
}

impl RuntimeProviderStatus {
    pub fn supports_class(&self, class: &str) -> bool {
        self.classes.iter().any(|candidate| candidate == class)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeStatus {
    #[serde(default)]
    pub capacity: ResourceList,
    #[serde(default)]
    pub allocatable: ResourceList,
    #[serde(default)]
    pub conditions: Vec<NodeCondition>,
    #[serde(default)]
    pub addresses: Vec<NodeAddress>,
    #[serde(default, rename = "nodeInfo")]
    pub node_info: NodeSystemInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<String>,
    #[serde(
        default,
        rename = "runtimeCapabilities",
        skip_serializing_if = "Option::is_none"
    )]
    pub runtime_capabilities: Option<RuntimeCapabilities>,
}

pub type Node = crate::Object<NodeSpec>;

impl Resource for NodeSpec {
    type Status = NodeStatus;
    const KIND: &'static str = "Node";
    const API_VERSION: &'static str = "boss.io/v1";
}
