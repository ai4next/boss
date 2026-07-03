use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Resource, meta::Labels};

/// A quantity string (e.g. "1", "512Mi"). Stored opaquely for the skeleton.
pub type Quantity = String;
pub type ResourceList = BTreeMap<String, Quantity>;

pub const DEFAULT_SANDBOX_CLASS: &str = "process";
pub const ANNOTATION_RESOLVED_SANDBOX_CLASS: &str = "boss.io/resolved-sandbox-class";
pub const ANNOTATION_SELECTED_PROVIDER: &str = "boss.io/selected-provider";

pub const REASON_UNSUPPORTED_CLASS: &str = "UnsupportedClass";
pub const REASON_PROVIDER_UNAVAILABLE: &str = "ProviderUnavailable";
pub const REASON_INVALID_SPEC: &str = "InvalidSpec";
pub const REASON_CREATE_FAILED: &str = "CreateFailed";
pub const REASON_START_FAILED: &str = "StartFailed";
pub const REASON_STATUS_UNKNOWN: &str = "StatusUnknown";
pub const REASON_UNSCHEDULABLE: &str = "Unschedulable";

pub fn normalize_sandbox_class(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "process" | "baremetal" | "bare-metal" | "metal" => Some("process"),
        "container" | "containerd" | "runc" => Some("container"),
        "wasm" | "wasmtime" => Some("wasm"),
        "microvm" | "firecracker" | "cloud-hypervisor" => Some("microvm"),
        "vm" | "qemu" | "libvirt" => Some("vm"),
        "remote" => Some("remote"),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRequirements {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests: Option<ResourceList>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<ResourceList>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RestartPolicy {
    Always,
    OnFailure,
    #[default]
    Never,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    #[default]
    Host,
    None,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<EnvVar>,
    #[serde(default)]
    pub resources: ResourceRequirements,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm_module: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxRequirements {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<SandboxArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<SandboxNetwork>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<SandboxSecurity>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxArtifact {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxNetwork {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxSecurity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privileged: Option<bool>,
    #[serde(
        default,
        rename = "readonlyRootfs",
        skip_serializing_if = "Option::is_none"
    )]
    pub readonly_rootfs: Option<bool>,
    #[serde(
        default,
        rename = "allowHostAccess",
        skip_serializing_if = "Option::is_none"
    )]
    pub allow_host_access: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

/// User-visible summary of a pod's lifecycle state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PodPhase {
    #[default]
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
}

impl std::fmt::Display for PodPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodCondition {
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStatus {
    pub name: String,
    pub ready: bool,
    #[serde(
        rename = "containerID",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub container_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ContainerState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerState {
    Running {
        started_at: String,
    },
    Terminated {
        finished_at: String,
        exit_code: i32,
        reason: Option<String>,
    },
    Waiting {
        reason: Option<String>,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodSpec {
    /// Node the pod is assigned to. `None` until the scheduler binds it.
    #[serde(default, rename = "nodeName", skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    /// Runtime class: "baremetal" | "container" | "vm" | "wasm".
    #[serde(
        default,
        rename = "runtimeClass",
        skip_serializing_if = "Option::is_none"
    )]
    pub runtime_class: Option<String>,
    /// Sandbox class: "process" | "container" | "wasm" | "microvm" | "vm" | "remote".
    #[serde(
        default,
        rename = "sandboxClass",
        skip_serializing_if = "Option::is_none"
    )]
    pub sandbox_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<SandboxRequirements>,
    #[serde(default)]
    pub containers: Vec<Container>,
    #[serde(default)]
    pub restart_policy: RestartPolicy,
    #[serde(default, rename = "terminationGracePeriodSeconds")]
    pub termination_grace_seconds: i64,
    #[serde(default)]
    pub resources: ResourceRequirements,
    #[serde(
        default,
        rename = "nodeSelector",
        skip_serializing_if = "Option::is_none"
    )]
    pub node_selector: Option<Labels>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tolerations: Vec<Toleration>,
}

impl PodSpec {
    pub fn resolved_sandbox_class(&self) -> String {
        self.sandbox_class
            .as_deref()
            .or(self.runtime_class.as_deref())
            .and_then(normalize_sandbox_class)
            .unwrap_or(DEFAULT_SANDBOX_CLASS)
            .to_string()
    }

    pub fn resolved_sandbox_intent(&self) -> ResolvedSandboxIntent {
        self.sandbox_intent_for_class(self.resolved_sandbox_class())
    }

    pub fn try_resolved_sandbox_intent(&self) -> Result<ResolvedSandboxIntent, SandboxIntentError> {
        let sandbox_class = self
            .sandbox_class
            .as_deref()
            .map(|value| {
                normalize_sandbox_class(value).ok_or_else(|| {
                    SandboxIntentError::new(
                        REASON_UNSUPPORTED_CLASS,
                        format!("unsupported sandboxClass {value}"),
                    )
                })
            })
            .transpose()?;
        let runtime_class = self
            .runtime_class
            .as_deref()
            .map(|value| {
                normalize_sandbox_class(value).ok_or_else(|| {
                    SandboxIntentError::new(
                        REASON_UNSUPPORTED_CLASS,
                        format!("unsupported runtimeClass {value}"),
                    )
                })
            })
            .transpose()?;

        if let (Some(sandbox_class), Some(runtime_class)) = (sandbox_class, runtime_class)
            && sandbox_class != runtime_class
        {
            return Err(SandboxIntentError::new(
                REASON_INVALID_SPEC,
                format!(
                    "sandboxClass resolves to {sandbox_class}, but runtimeClass resolves to {runtime_class}"
                ),
            ));
        }

        let class = sandbox_class
            .or(runtime_class)
            .unwrap_or(DEFAULT_SANDBOX_CLASS)
            .to_string();
        Ok(self.sandbox_intent_for_class(class))
    }

    fn sandbox_intent_for_class(&self, class: String) -> ResolvedSandboxIntent {
        let sandbox = self.sandbox.as_ref();
        ResolvedSandboxIntent {
            class,
            artifact_type: sandbox
                .and_then(|sandbox| sandbox.artifact.as_ref())
                .map(|artifact| artifact.kind.clone()),
            network_mode: sandbox
                .and_then(|sandbox| sandbox.network.as_ref())
                .and_then(|network| network.mode.clone()),
            isolation: sandbox.and_then(|sandbox| sandbox.isolation.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxIntentError {
    pub reason: &'static str,
    pub message: String,
}

impl SandboxIntentError {
    pub fn new(reason: &'static str, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SandboxIntentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for SandboxIntentError {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSandboxIntent {
    pub class: String,
    #[serde(
        default,
        rename = "artifactType",
        skip_serializing_if = "Option::is_none"
    )]
    pub artifact_type: Option<String>,
    #[serde(
        default,
        rename = "networkMode",
        skip_serializing_if = "Option::is_none"
    )]
    pub network_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<String>,
}

impl ResolvedSandboxIntent {
    pub fn unsupported_message(&self) -> String {
        let mut parts = vec![format!("class={}", self.class)];
        if let Some(artifact_type) = &self.artifact_type {
            parts.push(format!("artifactType={artifact_type}"));
        }
        if let Some(network_mode) = &self.network_mode {
            parts.push(format!("networkMode={network_mode}"));
        }
        if let Some(isolation) = &self.isolation {
            parts.push(format!("isolation={isolation}"));
        }
        format!(
            "no healthy provider supports sandbox intent ({})",
            parts.join(", ")
        )
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Toleration {
    pub key: String,
    pub operator: String,
    pub value: String,
    pub effect: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodStatus {
    #[serde(default)]
    pub phase: PodPhase,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<PodCondition>,
    #[serde(
        default,
        rename = "containerStatuses",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub container_statuses: Vec<ContainerStatus>,
    #[serde(default, rename = "hostIP", skip_serializing_if = "Option::is_none")]
    pub host_ip: Option<String>,
    #[serde(default, rename = "podIP", skip_serializing_if = "Option::is_none")]
    pub pod_ip: Option<String>,
    #[serde(default, rename = "startTime", skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(default, rename = "sandboxID", skip_serializing_if = "Option::is_none")]
    pub sandbox_id: Option<String>,
    #[serde(
        default,
        rename = "sandboxClass",
        skip_serializing_if = "Option::is_none"
    )]
    pub sandbox_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub type Pod = crate::Object<PodSpec>;

impl Resource for PodSpec {
    type Status = PodStatus;
    const KIND: &'static str = "Pod";
    const API_VERSION: &'static str = "boss.io/v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_sandbox_intent_from_pod_spec() {
        let spec = PodSpec {
            sandbox_class: Some("baremetal".to_string()),
            sandbox: Some(SandboxRequirements {
                artifact: Some(SandboxArtifact {
                    kind: "executable".to_string(),
                    path: Some("sleep".to_string()),
                    ..Default::default()
                }),
                network: Some(SandboxNetwork {
                    mode: Some("host".to_string()),
                }),
                isolation: Some("sharedHost".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(
            spec.resolved_sandbox_intent(),
            ResolvedSandboxIntent {
                class: "process".to_string(),
                artifact_type: Some("executable".to_string()),
                network_mode: Some("host".to_string()),
                isolation: Some("sharedHost".to_string()),
            }
        );
    }

    #[test]
    fn rejects_unknown_sandbox_class() {
        let spec = PodSpec {
            sandbox_class: Some("unknown".to_string()),
            ..Default::default()
        };

        let error = spec.try_resolved_sandbox_intent().unwrap_err();
        assert_eq!(error.reason, REASON_UNSUPPORTED_CLASS);
        assert_eq!(error.message, "unsupported sandboxClass unknown");
    }

    #[test]
    fn rejects_conflicting_sandbox_and_runtime_classes() {
        let spec = PodSpec {
            sandbox_class: Some("wasm".to_string()),
            runtime_class: Some("container".to_string()),
            ..Default::default()
        };

        let error = spec.try_resolved_sandbox_intent().unwrap_err();
        assert_eq!(error.reason, REASON_INVALID_SPEC);
        assert_eq!(
            error.message,
            "sandboxClass resolves to wasm, but runtimeClass resolves to container"
        );
    }
}
