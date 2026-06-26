use async_trait::async_trait;

use crate::runtime_trait::Runtime;
use crate::spec::{RuntimeCapabilities, SandboxId, SandboxSpec, SandboxStatus, SandboxSummary};

/// microVM runtime stub (firecracker/cloud-hypervisor). TODO(Phase 6).
pub struct VmRuntime;

#[async_trait]
impl Runtime for VmRuntime {
    fn name(&self) -> &'static str {
        "vm"
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::new(
            self.name(),
            false,
            vec!["vm", "microvm"],
            vec!["rootfsImage", "diskImage"],
            vec!["none", "nat", "tap"],
            vec!["hardwareVirtualized"],
        )
    }

    async fn create(&self, _spec: SandboxSpec) -> crate::RuntimeResult<SandboxId> {
        Err(boss_common::BossError::NotImplemented("vm runtime".into()))
    }
    async fn start(&self, _id: &SandboxId) -> crate::RuntimeResult<()> {
        Err(boss_common::BossError::NotImplemented("vm runtime".into()))
    }
    async fn stop(&self, _id: &SandboxId, _force: bool) -> crate::RuntimeResult<()> {
        Err(boss_common::BossError::NotImplemented("vm runtime".into()))
    }
    async fn remove(&self, _id: &SandboxId) -> crate::RuntimeResult<()> {
        Err(boss_common::BossError::NotImplemented("vm runtime".into()))
    }
    async fn status(&self, _id: &SandboxId) -> crate::RuntimeResult<SandboxStatus> {
        Err(boss_common::BossError::NotImplemented("vm runtime".into()))
    }
    async fn list(&self) -> crate::RuntimeResult<Vec<SandboxSummary>> {
        Ok(Vec::new())
    }
}
