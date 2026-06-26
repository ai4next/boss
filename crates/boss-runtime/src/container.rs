use async_trait::async_trait;

use crate::runtime_trait::Runtime;
use crate::spec::{RuntimeCapabilities, SandboxId, SandboxSpec, SandboxStatus, SandboxSummary};

/// Container runtime stub (containerd/runc). TODO(Phase 6): real impl via
/// containerd shim / runc CLI.
pub struct ContainerRuntime;

#[async_trait]
impl Runtime for ContainerRuntime {
    fn name(&self) -> &'static str {
        "container"
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::new(
            self.name(),
            false,
            vec!["container"],
            vec!["containerImage"],
            vec!["host", "none"],
            vec!["namespaced"],
        )
    }

    async fn create(&self, _spec: SandboxSpec) -> crate::RuntimeResult<SandboxId> {
        Err(boss_common::BossError::NotImplemented(
            "container runtime".into(),
        ))
    }
    async fn start(&self, _id: &SandboxId) -> crate::RuntimeResult<()> {
        Err(boss_common::BossError::NotImplemented(
            "container runtime".into(),
        ))
    }
    async fn stop(&self, _id: &SandboxId, _force: bool) -> crate::RuntimeResult<()> {
        Err(boss_common::BossError::NotImplemented(
            "container runtime".into(),
        ))
    }
    async fn remove(&self, _id: &SandboxId) -> crate::RuntimeResult<()> {
        Err(boss_common::BossError::NotImplemented(
            "container runtime".into(),
        ))
    }
    async fn status(&self, _id: &SandboxId) -> crate::RuntimeResult<SandboxStatus> {
        Err(boss_common::BossError::NotImplemented(
            "container runtime".into(),
        ))
    }
    async fn list(&self) -> crate::RuntimeResult<Vec<SandboxSummary>> {
        Ok(Vec::new())
    }
}
