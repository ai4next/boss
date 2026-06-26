//! CRI-style runtime abstraction with multiple providers. BareMetal is a real
//! process-spawning implementation (proves end-to-end); container/vm/wasm are
//! stubs returning `NotImplemented` (Phase 6).

pub mod baremetal;
pub mod container;
pub mod error;
pub mod manager;
pub mod runtime_trait;
pub mod spec;
pub mod vm;
pub mod wasm;

pub use baremetal::BareMetalRuntime;
pub use container::ContainerRuntime;
pub use error::{RuntimeError, RuntimeResult};
pub use manager::RuntimeManager;
pub use runtime_trait::Runtime;
pub use spec::{
    NetworkMode, RuntimeCapabilities, RuntimeClass, SandboxClass, SandboxId, SandboxSpec,
    SandboxStatus, SandboxSummary,
};
pub use vm::VmRuntime;
pub use wasm::WasmRuntime;
