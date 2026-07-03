use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::process::{Child, Command};

use crate::runtime_trait::Runtime;
use crate::spec::{RuntimeCapabilities, SandboxId, SandboxSpec, SandboxStatus, SandboxSummary};

struct WasmSandbox {
    spec: SandboxSpec,
    child: Mutex<Option<Child>>,
    started_at: Mutex<Option<String>>,
}

/// WASM runtime backed by the local `wasmtime` CLI.
pub struct WasmRuntime {
    available: bool,
    sandboxes: DashMap<SandboxId, WasmSandbox>,
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmRuntime {
    pub fn new() -> Self {
        Self {
            available: command_exists("wasmtime"),
            sandboxes: DashMap::new(),
        }
    }

    fn ensure_available(&self) -> crate::RuntimeResult<()> {
        if self.available {
            Ok(())
        } else {
            Err(boss_common::BossError::NotImplemented(
                "wasm runtime requires wasmtime".into(),
            ))
        }
    }
}

#[async_trait]
impl Runtime for WasmRuntime {
    fn name(&self) -> &'static str {
        "wasm"
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::with_reason(
            self.name(),
            self.available,
            (!self.available).then_some("MissingWasmtime"),
            vec!["wasm"],
            vec!["wasmModule"],
            vec!["none"],
            vec!["runtimeSandboxed"],
        )
    }

    async fn create(&self, spec: SandboxSpec) -> crate::RuntimeResult<SandboxId> {
        self.ensure_available()?;
        if wasm_module_path(&spec).is_none() {
            return Err(boss_common::BossError::invalid(
                "wasm runtime requires a local wasm module path",
            ));
        }
        let id = boss_common::id::short_id();
        self.sandboxes.insert(
            id.clone(),
            WasmSandbox {
                spec,
                child: Mutex::new(None),
                started_at: Mutex::new(None),
            },
        );
        Ok(id)
    }

    async fn start(&self, id: &SandboxId) -> crate::RuntimeResult<()> {
        self.ensure_available()?;
        let (module, args, env) = {
            let entry = self
                .sandboxes
                .get(id)
                .ok_or_else(|| boss_common::BossError::NotFound(format!("sandbox {id}")))?;
            (
                wasm_module_path(&entry.spec).ok_or_else(|| {
                    boss_common::BossError::invalid(
                        "wasm runtime requires a local wasm module path",
                    )
                })?,
                entry.spec.args.clone(),
                entry
                    .spec
                    .env
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<Vec<_>>(),
            )
        };

        let mut cmd = Command::new("wasmtime");
        cmd.arg(module);
        cmd.args(args);
        for (key, value) in env {
            cmd.env(key, value);
        }
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.kill_on_drop(true);

        let child = cmd.spawn().map_err(|error| {
            boss_common::BossError::Internal(format!("wasm runtime spawn wasmtime: {error}"))
        })?;
        let entry = self
            .sandboxes
            .get(id)
            .ok_or_else(|| boss_common::BossError::NotFound(format!("sandbox {id}")))?;
        *entry.child.lock() = Some(child);
        *entry.started_at.lock() = Some(boss_common::time::now_rfc3339());
        Ok(())
    }

    async fn stop(&self, id: &SandboxId, _force: bool) -> crate::RuntimeResult<()> {
        let child = {
            let entry = self
                .sandboxes
                .get(id)
                .ok_or_else(|| boss_common::BossError::NotFound(format!("sandbox {id}")))?;
            entry.child.lock().take()
        };
        if let Some(mut child) = child {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        Ok(())
    }

    async fn remove(&self, id: &SandboxId) -> crate::RuntimeResult<()> {
        if self.sandboxes.remove(id).is_some() {
            Ok(())
        } else {
            Err(boss_common::BossError::NotFound(format!("sandbox {id}")))
        }
    }

    async fn status(&self, id: &SandboxId) -> crate::RuntimeResult<SandboxStatus> {
        let entry = self
            .sandboxes
            .get(id)
            .ok_or_else(|| boss_common::BossError::NotFound(format!("sandbox {id}")))?;
        let (running, exit_code) = {
            let mut guard = entry.child.lock();
            match guard.as_mut() {
                None => (false, None),
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        let code = status.code();
                        *guard = None;
                        (false, code)
                    }
                    Ok(None) => (true, None),
                    Err(_) => (false, None),
                },
            }
        };
        Ok(SandboxStatus {
            id: id.clone(),
            running,
            exit_code,
            started_at: entry.started_at.lock().clone(),
            finished_at: None,
        })
    }

    async fn list(&self) -> crate::RuntimeResult<Vec<SandboxSummary>> {
        let mut out = Vec::new();
        for entry in self.sandboxes.iter() {
            let running = entry
                .child
                .lock()
                .as_mut()
                .is_some_and(|child| child.try_wait().ok().flatten().is_none());
            out.push(SandboxSummary {
                id: entry.key().clone(),
                pod_uid: entry.spec.pod_uid.clone(),
                running,
            });
        }
        Ok(out)
    }
}

fn wasm_module_path(spec: &SandboxSpec) -> Option<PathBuf> {
    spec.artifact_path
        .as_ref()
        .or(spec.artifact_uri.as_ref())
        .map(PathBuf::from)
        .filter(|path| path.exists())
}

fn command_exists(command: &str) -> bool {
    std::process::Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn capabilities_reflect_wasmtime_availability() {
        let runtime = WasmRuntime::new();
        let capabilities = runtime.capabilities().await.provider;

        assert_eq!(capabilities.name, "wasm");
        assert_eq!(capabilities.healthy, runtime.available);
        assert!(capabilities.classes.contains(&"wasm".to_string()));
    }
}
