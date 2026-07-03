use std::process::Stdio;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::process::{Child, Command};

use crate::runtime_trait::Runtime;
use crate::spec::{
    NetworkMode, RuntimeCapabilities, SandboxId, SandboxSpec, SandboxStatus, SandboxSummary,
};

struct ContainerSandbox {
    spec: SandboxSpec,
    child: Mutex<Option<Child>>,
    started_at: Mutex<Option<String>>,
}

/// Container runtime backed by a local `docker` or `podman` CLI.
pub struct ContainerRuntime {
    engine: Option<&'static str>,
    sandboxes: DashMap<SandboxId, ContainerSandbox>,
}

impl Default for ContainerRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerRuntime {
    pub fn new() -> Self {
        Self {
            engine: find_engine(),
            sandboxes: DashMap::new(),
        }
    }

    fn engine(&self) -> crate::RuntimeResult<&'static str> {
        self.engine.ok_or_else(|| {
            boss_common::BossError::NotImplemented(
                "container runtime requires docker or podman".into(),
            )
        })
    }
}

#[async_trait]
impl Runtime for ContainerRuntime {
    fn name(&self) -> &'static str {
        "container"
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::with_reason(
            self.name(),
            self.engine.is_some(),
            self.engine.is_none().then_some("MissingContainerEngine"),
            vec!["container"],
            vec!["containerImage"],
            vec!["host", "none"],
            vec!["namespaced"],
        )
    }

    async fn create(&self, spec: SandboxSpec) -> crate::RuntimeResult<SandboxId> {
        self.engine()?;
        if spec.image.as_deref().unwrap_or_default().is_empty() {
            return Err(boss_common::BossError::invalid(
                "container runtime requires an image",
            ));
        }
        let id = format!("boss-{}", boss_common::id::short_id());
        self.sandboxes.insert(
            id.clone(),
            ContainerSandbox {
                spec,
                child: Mutex::new(None),
                started_at: Mutex::new(None),
            },
        );
        Ok(id)
    }

    async fn start(&self, id: &SandboxId) -> crate::RuntimeResult<()> {
        let engine = self.engine()?;
        let (image, command, args, env, network) = {
            let entry = self
                .sandboxes
                .get(id)
                .ok_or_else(|| boss_common::BossError::NotFound(format!("sandbox {id}")))?;
            (
                entry.spec.image.clone().ok_or_else(|| {
                    boss_common::BossError::invalid("container runtime requires an image")
                })?,
                entry.spec.command.clone(),
                entry.spec.args.clone(),
                entry
                    .spec
                    .env
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<Vec<_>>(),
                entry.spec.network,
            )
        };

        let mut cmd = Command::new(engine);
        cmd.arg("run")
            .arg("--rm")
            .arg("--name")
            .arg(id)
            .arg("--network")
            .arg(match network {
                NetworkMode::Host => "host",
                NetworkMode::None => "none",
            });
        for (key, value) in env {
            cmd.arg("-e").arg(format!("{key}={value}"));
        }
        cmd.arg(image);
        cmd.args(command);
        cmd.args(args);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.kill_on_drop(true);

        let child = cmd.spawn().map_err(|error| {
            boss_common::BossError::Internal(format!("container runtime spawn {engine}: {error}"))
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
        if let Some(engine) = self.engine {
            let _ = Command::new(engine)
                .arg("rm")
                .arg("-f")
                .arg(id)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
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

fn find_engine() -> Option<&'static str> {
    if command_exists("docker") {
        Some("docker")
    } else if command_exists("podman") {
        Some("podman")
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
    async fn capabilities_reflect_engine_availability() {
        let runtime = ContainerRuntime::new();
        let capabilities = runtime.capabilities().await.provider;

        assert_eq!(capabilities.name, "container");
        assert_eq!(capabilities.healthy, runtime.engine.is_some());
        assert!(capabilities.classes.contains(&"container".to_string()));
    }
}
