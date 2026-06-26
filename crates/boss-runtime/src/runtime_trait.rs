use async_trait::async_trait;

use crate::spec::{RuntimeCapabilities, SandboxId, SandboxSpec, SandboxStatus, SandboxSummary};

/// CRI-style runtime provider. Each provider (baremetal/container/vm/wasm)
/// implements this; the bosslet talks to providers through `RuntimeManager`.
#[async_trait]
pub trait Runtime: Send + Sync {
    fn name(&self) -> &'static str;

    async fn capabilities(&self) -> RuntimeCapabilities;

    /// Create the sandbox (does not start it). Returns the assigned id.
    async fn create(&self, spec: SandboxSpec) -> crate::RuntimeResult<SandboxId>;

    /// Start a created sandbox.
    async fn start(&self, id: &SandboxId) -> crate::RuntimeResult<()>;

    /// Stop a running sandbox. `force` requests SIGKILL vs graceful.
    async fn stop(&self, id: &SandboxId, force: bool) -> crate::RuntimeResult<()>;

    /// Remove a sandbox (must be stopped).
    async fn remove(&self, id: &SandboxId) -> crate::RuntimeResult<()>;

    /// Current status of a sandbox.
    async fn status(&self, id: &SandboxId) -> crate::RuntimeResult<SandboxStatus>;

    /// List known sandboxes.
    async fn list(&self) -> crate::RuntimeResult<Vec<SandboxSummary>>;
}
