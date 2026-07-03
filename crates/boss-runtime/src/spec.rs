use std::collections::BTreeMap;

use boss_api::RuntimeProviderStatus;
use serde::{Deserialize, Serialize};

/// Which runtime provider backs a sandbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeClass {
    BareMetal,
    Container,
    Vm,
    Wasm,
}

impl RuntimeClass {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "baremetal" | "bare-metal" | "metal" => Some(Self::BareMetal),
            "container" | "containerd" | "runc" => Some(Self::Container),
            "vm" | "microvm" | "firecracker" => Some(Self::Vm),
            "wasm" | "wasmtime" => Some(Self::Wasm),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BareMetal => "process",
            Self::Container => "container",
            Self::Vm => "vm",
            Self::Wasm => "wasm",
        }
    }
}

pub type SandboxClass = String;

#[derive(Clone, Debug)]
pub struct RuntimeCapabilities {
    pub provider: RuntimeProviderStatus,
}

impl RuntimeCapabilities {
    pub fn new(
        name: impl Into<String>,
        healthy: bool,
        classes: Vec<&str>,
        artifact_types: Vec<&str>,
        network_modes: Vec<&str>,
        isolation_levels: Vec<&str>,
    ) -> Self {
        Self::with_reason(
            name,
            healthy,
            if healthy { None } else { Some("Unavailable") },
            classes,
            artifact_types,
            network_modes,
            isolation_levels,
        )
    }

    pub fn with_reason(
        name: impl Into<String>,
        healthy: bool,
        reason: Option<&str>,
        classes: Vec<&str>,
        artifact_types: Vec<&str>,
        network_modes: Vec<&str>,
        isolation_levels: Vec<&str>,
    ) -> Self {
        Self {
            provider: RuntimeProviderStatus {
                name: name.into(),
                healthy,
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                reason: reason.map(str::to_string),
                classes: classes.into_iter().map(str::to_string).collect(),
                artifact_types: artifact_types.into_iter().map(str::to_string).collect(),
                network_modes: network_modes.into_iter().map(str::to_string).collect(),
                isolation_levels: isolation_levels.into_iter().map(str::to_string).collect(),
            },
        }
    }
}

pub type SandboxId = String;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    #[default]
    Host,
    None,
}

/// Desired state of a sandbox, derived from a Pod by the bosslet.
#[derive(Clone, Debug)]
pub struct SandboxSpec {
    pub pod_uid: String,
    pub pod_name: String,
    pub namespace: String,
    pub runtime_class: RuntimeClass,
    pub sandbox_class: SandboxClass,
    pub provider: Option<String>,
    /// Entrypoint command; `command[0]` is the program, the rest are args.
    pub command: Vec<String>,
    /// Extra args appended after `command`.
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub image: Option<String>,
    pub artifact_type: Option<String>,
    pub artifact_uri: Option<String>,
    pub artifact_path: Option<String>,
    pub wasm_module: Option<Vec<u8>>,
    pub network: NetworkMode,
}

/// Observed state of a sandbox.
#[derive(Clone, Debug)]
pub struct SandboxStatus {
    pub id: SandboxId,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SandboxSummary {
    pub id: SandboxId,
    pub pod_uid: String,
    pub running: bool,
}
