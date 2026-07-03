use serde::{Deserialize, Serialize};

use crate::{ResolvedSandboxIntent, Resource, ResourceList, meta::Labels};

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

    pub fn healthy_provider_for_intent(
        &self,
        intent: &ResolvedSandboxIntent,
    ) -> Option<&RuntimeProviderStatus> {
        self.providers
            .iter()
            .find(|provider| provider.healthy && provider.supports_sandbox_intent(intent))
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

    pub fn supports_artifact_type(&self, artifact_type: Option<&str>) -> bool {
        artifact_type
            .map(|required| supports_optional_value(&self.artifact_types, required))
            .unwrap_or(true)
    }

    pub fn supports_network_mode(&self, network_mode: Option<&str>) -> bool {
        network_mode
            .map(|required| supports_optional_value(&self.network_modes, required))
            .unwrap_or(true)
    }

    pub fn supports_isolation_level(&self, isolation_level: Option<&str>) -> bool {
        isolation_level
            .map(|required| supports_optional_value(&self.isolation_levels, required))
            .unwrap_or(true)
    }

    pub fn supports_sandbox_intent(&self, intent: &ResolvedSandboxIntent) -> bool {
        self.supports_class(&intent.class)
            && self.supports_artifact_type(intent.artifact_type.as_deref())
            && self.supports_network_mode(intent.network_mode.as_deref())
            && self.supports_isolation_level(intent.isolation.as_deref())
    }
}

fn supports_optional_value(supported: &[String], required: &str) -> bool {
    supported
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(required))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> RuntimeProviderStatus {
        RuntimeProviderStatus {
            name: "baremetal".to_string(),
            healthy: true,
            classes: vec!["process".to_string()],
            artifact_types: vec!["executable".to_string()],
            network_modes: vec!["host".to_string(), "none".to_string()],
            isolation_levels: vec!["sharedHost".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn provider_matches_full_intent_case_insensitively() {
        let intent = ResolvedSandboxIntent {
            class: "process".to_string(),
            artifact_type: Some("EXECUTABLE".to_string()),
            network_mode: Some("Host".to_string()),
            isolation: Some("sharedhost".to_string()),
        };

        assert!(provider().supports_sandbox_intent(&intent));
    }

    #[test]
    fn provider_rejects_unsupported_intent_dimension() {
        let intent = ResolvedSandboxIntent {
            class: "process".to_string(),
            artifact_type: Some("wasmModule".to_string()),
            network_mode: Some("host".to_string()),
            isolation: Some("sharedHost".to_string()),
        };

        assert!(!provider().supports_sandbox_intent(&intent));
    }
}
