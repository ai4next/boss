use async_trait::async_trait;

use crate::runtime_trait::Runtime;
use crate::spec::{RuntimeCapabilities, SandboxId, SandboxSpec, SandboxStatus, SandboxSummary};

/// WASM runtime stub (wasmtime). TODO(Phase 6): real impl behind the `wasm`
/// feature using the wasmtime API.
pub struct WasmRuntime;

#[async_trait]
impl Runtime for WasmRuntime {
    fn name(&self) -> &'static str {
        "wasm"
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::new(
            self.name(),
            false,
            vec!["wasm"],
            vec!["wasmModule", "ociWasm"],
            vec!["none"],
            vec!["runtimeSandboxed"],
        )
    }

    async fn create(&self, _spec: SandboxSpec) -> crate::RuntimeResult<SandboxId> {
        Err(boss_common::BossError::NotImplemented(
            "wasm runtime".into(),
        ))
    }
    async fn start(&self, _id: &SandboxId) -> crate::RuntimeResult<()> {
        Err(boss_common::BossError::NotImplemented(
            "wasm runtime".into(),
        ))
    }
    async fn stop(&self, _id: &SandboxId, _force: bool) -> crate::RuntimeResult<()> {
        Err(boss_common::BossError::NotImplemented(
            "wasm runtime".into(),
        ))
    }
    async fn remove(&self, _id: &SandboxId) -> crate::RuntimeResult<()> {
        Err(boss_common::BossError::NotImplemented(
            "wasm runtime".into(),
        ))
    }
    async fn status(&self, _id: &SandboxId) -> crate::RuntimeResult<SandboxStatus> {
        Err(boss_common::BossError::NotImplemented(
            "wasm runtime".into(),
        ))
    }
    async fn list(&self) -> crate::RuntimeResult<Vec<SandboxSummary>> {
        Ok(Vec::new())
    }
}
