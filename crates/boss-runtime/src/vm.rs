use std::process::Stdio;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::process::{Child, Command};

use crate::runtime_trait::Runtime;
use crate::spec::{RuntimeCapabilities, SandboxId, SandboxSpec, SandboxStatus, SandboxSummary};

struct VmSandbox {
    spec: SandboxSpec,
    child: Mutex<Option<Child>>,
    started_at: Mutex<Option<String>>,
}

/// VM/microVM runtime backed by local `qemu-system-*` binaries.
pub struct VmRuntime {
    engine: Option<&'static str>,
    sandboxes: DashMap<SandboxId, VmSandbox>,
}

impl Default for VmRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl VmRuntime {
    pub fn new() -> Self {
        Self {
            engine: find_qemu(),
            sandboxes: DashMap::new(),
        }
    }

    fn engine(&self) -> crate::RuntimeResult<&'static str> {
        self.engine.ok_or_else(|| {
            boss_common::BossError::NotImplemented(
                "vm runtime requires qemu-system-aarch64 or qemu-system-x86_64".into(),
            )
        })
    }
}

#[async_trait]
impl Runtime for VmRuntime {
    fn name(&self) -> &'static str {
        "vm"
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::with_reason(
            self.name(),
            self.engine.is_some(),
            self.engine.is_none().then_some("MissingQemu"),
            vec!["vm", "microvm"],
            vec!["diskImage", "rootfsImage"],
            vec!["none"],
            vec!["hardwareVirtualized"],
        )
    }

    async fn create(&self, spec: SandboxSpec) -> crate::RuntimeResult<SandboxId> {
        self.engine()?;
        if spec.artifact_path.as_deref().unwrap_or_default().is_empty()
            && spec.image.as_deref().unwrap_or_default().is_empty()
        {
            return Err(boss_common::BossError::invalid(
                "vm runtime requires a local disk/rootfs image path",
            ));
        }
        let id = boss_common::id::short_id();
        self.sandboxes.insert(
            id.clone(),
            VmSandbox {
                spec,
                child: Mutex::new(None),
                started_at: Mutex::new(None),
            },
        );
        Ok(id)
    }

    async fn start(&self, id: &SandboxId) -> crate::RuntimeResult<()> {
        let engine = self.engine()?;
        let disk = {
            let entry = self
                .sandboxes
                .get(id)
                .ok_or_else(|| boss_common::BossError::NotFound(format!("sandbox {id}")))?;
            entry
                .spec
                .artifact_path
                .clone()
                .or_else(|| entry.spec.image.clone())
                .ok_or_else(|| {
                    boss_common::BossError::invalid(
                        "vm runtime requires a local disk/rootfs image path",
                    )
                })?
        };

        let mut cmd = Command::new(engine);
        cmd.arg("-display")
            .arg("none")
            .arg("-serial")
            .arg("none")
            .arg("-monitor")
            .arg("none")
            .arg("-no-reboot")
            .arg("-m")
            .arg("256M")
            .arg("-drive")
            .arg(format!("file={disk},format=raw,if=virtio"));
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.kill_on_drop(true);

        let child = cmd.spawn().map_err(|error| {
            boss_common::BossError::Internal(format!("vm runtime spawn {engine}: {error}"))
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
        let child = self
            .sandboxes
            .get(id)
            .and_then(|entry| entry.child.lock().take());
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

fn find_qemu() -> Option<&'static str> {
    if command_exists("qemu-system-aarch64") {
        Some("qemu-system-aarch64")
    } else if command_exists("qemu-system-x86_64") {
        Some("qemu-system-x86_64")
    } else {
        None
    }
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
    async fn capabilities_reflect_qemu_availability() {
        let runtime = VmRuntime::new();
        let capabilities = runtime.capabilities().await.provider;

        assert_eq!(capabilities.name, "vm");
        assert_eq!(capabilities.healthy, runtime.engine.is_some());
        assert!(capabilities.classes.contains(&"microvm".to_string()));
    }
}
