use std::process::Stdio;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::process::{Child, Command};

use crate::runtime_trait::Runtime;
use crate::spec::{RuntimeCapabilities, SandboxId, SandboxSpec, SandboxStatus, SandboxSummary};

struct BareSandbox {
    spec: SandboxSpec,
    child: Mutex<Option<Child>>,
    started_at: Mutex<Option<String>>,
}

/// BareMetal runtime: spawns the configured command as a host process via
/// `tokio::process::Command`. This is the only fully-wired provider in the
/// skeleton — it makes the end-to-end flow (pod → process) observable.
pub struct BareMetalRuntime {
    sandboxes: DashMap<SandboxId, BareSandbox>,
}

impl Default for BareMetalRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl BareMetalRuntime {
    pub fn new() -> Self {
        Self {
            sandboxes: DashMap::new(),
        }
    }
}

/// Snapshot of what `start` needs from a sandbox, extracted under a short-lived
/// dashmap ref so no guard crosses an await point.
struct StartSnapshot {
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

#[async_trait]
impl Runtime for BareMetalRuntime {
    fn name(&self) -> &'static str {
        "baremetal"
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::new(
            self.name(),
            true,
            vec!["process"],
            vec!["executable"],
            vec!["host", "none"],
            vec!["sharedHost"],
        )
    }

    async fn create(&self, spec: SandboxSpec) -> crate::RuntimeResult<SandboxId> {
        let id = boss_common::id::short_id();
        self.sandboxes.insert(
            id.clone(),
            BareSandbox {
                spec,
                child: Mutex::new(None),
                started_at: Mutex::new(None),
            },
        );
        tracing::info!(sandbox = %id, "baremetal: created");
        Ok(id)
    }

    async fn start(&self, id: &SandboxId) -> crate::RuntimeResult<()> {
        let snap =
            {
                let entry = self
                    .sandboxes
                    .get(id)
                    .ok_or_else(|| boss_common::BossError::NotFound(format!("sandbox {id}")))?;
                let program =
                    entry.spec.command.first().cloned().ok_or_else(|| {
                        boss_common::BossError::invalid("baremetal: empty command")
                    })?;
                let mut args = entry.spec.command[1..].to_vec();
                args.extend_from_slice(&entry.spec.args);
                StartSnapshot {
                    program,
                    args,
                    env: entry
                        .spec
                        .env
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                }
            };

        let mut cmd = Command::new(&snap.program);
        cmd.args(&snap.args);
        for (k, v) in &snap.env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.kill_on_drop(true);

        let child = cmd.spawn().map_err(|e| {
            boss_common::BossError::Internal(format!("baremetal: spawn {}: {e}", snap.program))
        })?;

        {
            let entry = self
                .sandboxes
                .get(id)
                .ok_or_else(|| boss_common::BossError::NotFound(format!("sandbox {id}")))?;
            *entry.child.lock() = Some(child);
            *entry.started_at.lock() = Some(boss_common::time::now_rfc3339());
        }
        tracing::info!(sandbox = %id, program = %snap.program, "baremetal: started");
        Ok(())
    }

    async fn stop(&self, id: &SandboxId, _force: bool) -> crate::RuntimeResult<()> {
        // Take the child out under a short lock, then await kill/wait on the
        // owned child — no guard held across an await.
        let child_opt = {
            let entry = self
                .sandboxes
                .get(id)
                .ok_or_else(|| boss_common::BossError::NotFound(format!("sandbox {id}")))?;
            entry.child.lock().take()
        };
        if let Some(mut child) = child_opt {
            // Best-effort: SIGKILL always; a graceful path would SIGTERM first.
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        tracing::info!(sandbox = %id, "baremetal: stopped");
        Ok(())
    }

    async fn remove(&self, id: &SandboxId) -> crate::RuntimeResult<()> {
        if self.sandboxes.remove(id).is_some() {
            tracing::info!(sandbox = %id, "baremetal: removed");
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
                .is_some_and(|c| c.try_wait().ok().flatten().is_none());
            out.push(SandboxSummary {
                id: entry.key().clone(),
                pod_uid: entry.spec.pod_uid.clone(),
                running,
            });
        }
        Ok(out)
    }
}
