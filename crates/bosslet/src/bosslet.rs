use std::sync::Arc;

use anyhow::{Context, Result};
use boss_api::{
    ANNOTATION_SELECTED_PROVIDER, ContainerState, ContainerStatus, EventType, Node, NodeCondition,
    NodeSpec, NodeStatus, Object, ObjectMeta, Pod, PodPhase, PodSpec, REASON_CREATE_FAILED,
    REASON_PROVIDER_UNAVAILABLE, REASON_START_FAILED, REASON_UNSUPPORTED_CLASS, ResourceVersion,
    TypeMeta, WatchEvent, normalize_sandbox_class,
};
use boss_runtime::{NetworkMode, RuntimeClass, RuntimeManager, SandboxSpec};
use dashmap::DashMap;
use tokio_stream::StreamExt;

use crate::client::ApiServerClient;

/// Per-pod runtime state kept by the bosslet.
#[derive(Default)]
struct PodState {
    uid: String,
    sandbox_id: Option<String>,
    started: bool,
    terminated: bool,
    class: Option<RuntimeClass>,
    sandbox_class: Option<String>,
    provider: Option<String>,
    /// Last phase the bosslet successfully reported. Used to suppress the
    /// feedback loop where our own status write re-triggers sync.
    reported_phase: Option<PodPhase>,
}

#[derive(Clone, Debug)]
struct PodStatusUpdate {
    phase: PodPhase,
    reason: Option<String>,
    message: Option<String>,
    sandbox_id: Option<String>,
    sandbox_class: Option<String>,
    provider: Option<String>,
}

impl PodStatusUpdate {
    fn new(phase: PodPhase) -> Self {
        Self {
            phase,
            reason: None,
            message: None,
            sandbox_id: None,
            sandbox_class: None,
            provider: None,
        }
    }
}

/// Node agent: watches pods bound to this node, drives the runtime, reports
/// status, and heartbeats the node object.
pub struct Bosslet {
    node_name: String,
    client: ApiServerClient,
    runtime: RuntimeManager,
    pods: Arc<DashMap<String, PodState>>,
}

impl Bosslet {
    pub fn new(node_name: String, client: ApiServerClient, runtime: RuntimeManager) -> Self {
        Self {
            node_name,
            client,
            runtime,
            pods: Arc::new(DashMap::new()),
        }
    }

    pub async fn run(self: Arc<Self>) -> Result<()> {
        self.register_node().await?;

        let heartbeat = {
            let me = self.clone();
            tokio::spawn(async move { me.heartbeat_loop().await })
        };
        let pleg = {
            let me = self.clone();
            tokio::spawn(async move { me.pleg_loop().await })
        };
        let pod_resync = {
            let me = self.clone();
            tokio::spawn(async move { me.pod_resync_loop().await })
        };

        self.watch_loop().await?;

        heartbeat.abort();
        pleg.abort();
        pod_resync.abort();
        Ok(())
    }

    // ---- Node registration + heartbeat ----

    async fn register_node(&self) -> Result<()> {
        let node = self.initial_node().await;
        match self.client.get_node(&self.node_name).await? {
            None => {
                self.client.create_node(&node).await?;
                tracing::info!(node = %self.node_name, "registered node");
            }
            Some(existing) => {
                let mut n = existing;
                n.spec = node.spec.clone();
                n.status = node.status.clone();
                let mut n = self.client.update_node(&self.node_name, &n).await?;
                n.status = node.status.clone();
                self.client.update_node_status(&self.node_name, &n).await?;
                tracing::info!(node = %self.node_name, "updated existing node");
            }
        }
        Ok(())
    }

    async fn heartbeat_loop(&self) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            tick.tick().await;
            if let Err(e) = self.heartbeat_once().await {
                tracing::warn!(error = %e, "heartbeat failed");
            }
        }
    }

    async fn heartbeat_once(&self) -> Result<()> {
        for _ in 0..5 {
            let mut node = self
                .client
                .get_node(&self.node_name)
                .await?
                .context("node disappeared")?;
            let status = node.status.get_or_insert_default();
            set_ready(status);
            status.heartbeat = Some(boss_common::time::now_rfc3339());
            status.runtime_capabilities = Some(self.runtime.all_capabilities().await);
            match self.client.update_node_status(&self.node_name, &node).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    tracing::debug!(error = %e, "heartbeat CAS retry");
                    continue;
                }
            }
        }
        Err(anyhow::anyhow!("heartbeat exhausted retries"))
    }

    async fn initial_node(&self) -> Node {
        let mut capacity = std::collections::BTreeMap::new();
        capacity.insert("cpu".to_string(), "1".to_string());
        capacity.insert("memory".to_string(), "1Gi".to_string());
        Node {
            type_meta: TypeMeta {
                api_version: "boss.io/v1".into(),
                kind: "Node".into(),
            },
            metadata: ObjectMeta {
                name: self.node_name.clone(),
                ..Default::default()
            },
            spec: NodeSpec {
                provider: Some("baremetal".into()),
                ..Default::default()
            },
            status: Some({
                let mut status = NodeStatus {
                    capacity: capacity.clone(),
                    allocatable: capacity,
                    ..Default::default()
                };
                set_ready(&mut status);
                status.runtime_capabilities = Some(self.runtime.all_capabilities().await);
                status
            }),
        }
    }

    // ---- Watch loop ----

    async fn watch_loop(&self) -> Result<()> {
        loop {
            if let Err(e) = self.watch_once().await {
                tracing::warn!(error = %e, "watch stream ended, reconnecting");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }

    async fn watch_once(&self) -> Result<()> {
        let resp = self.client.watch_pods_raw(ResourceVersion(0)).await?;
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("watch stream error")?;
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line).trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let ev: WatchEvent = match serde_json::from_str(&line) {
                    Ok(ev) => ev,
                    Err(e) => {
                        tracing::warn!(error = %e, "unparseable watch line");
                        continue;
                    }
                };
                let obj = ev.object.clone();
                let kind = ev.kind;
                if let Err(e) = self.handle_event(obj, kind).await {
                    tracing::warn!(error = %e, "handle watch event failed");
                }
            }
        }
        Ok(())
    }

    async fn handle_event(&self, obj: serde_json::Value, kind: EventType) -> Result<()> {
        let pod: Pod = serde_json::from_value(obj)?;
        // Only handle pods bound to this node.
        if pod.spec.node_name.as_deref() != Some(self.node_name.as_str()) {
            return Ok(());
        }
        match kind {
            EventType::Added | EventType::Modified => self.sync_pod(pod).await,
            EventType::Deleted => self.sync_terminating(&pod).await,
        }
    }

    // ---- Pod sync ----

    async fn sync_pod(&self, pod: Pod) -> Result<()> {
        let key = format!("{}/{}", pod.metadata.namespace, pod.metadata.name);
        let uid = pod.metadata.uid.clone().unwrap_or_default();

        // Teardown if uid changed (recreated with same name).
        if let Some(existing) = self.pods.get(&key)
            && !existing.uid.is_empty()
            && existing.uid != uid
        {
            drop(existing);
            self.teardown(&key).await;
        }

        if pod.metadata.deletion_timestamp.is_some() {
            self.teardown(&key).await;
            return Ok(());
        }

        let selected_provider = selected_provider_for_pod(&pod);
        let intent = match pod.spec.try_resolved_sandbox_intent() {
            Ok(intent) => intent,
            Err(error) => {
                self.report_status(
                    &pod,
                    &key,
                    PodStatusUpdate {
                        reason: Some(error.reason.to_string()),
                        message: Some(error.message),
                        provider: selected_provider.clone(),
                        ..PodStatusUpdate::new(PodPhase::Failed)
                    },
                )
                .await;
                return Ok(());
            }
        };
        let sandbox_class = intent.class.clone();
        let class = runtime_class_for_sandbox(&sandbox_class).unwrap_or(RuntimeClass::BareMetal);

        let provider = match selected_provider.as_deref() {
            Some(name) => match self.runtime.provider_by_name(name) {
                Some(provider) => Some(provider),
                None => {
                    let msg = format!("selected provider {name} is not registered on this node");
                    self.report_status(
                        &pod,
                        &key,
                        PodStatusUpdate {
                            reason: Some(REASON_PROVIDER_UNAVAILABLE.to_string()),
                            message: Some(msg),
                            sandbox_class: Some(sandbox_class.clone()),
                            provider: Some(name.to_string()),
                            ..PodStatusUpdate::new(PodPhase::Failed)
                        },
                    )
                    .await;
                    return Ok(());
                }
            },
            None => self.runtime.default_provider_for_class(&sandbox_class),
        };

        let provider = match provider {
            Some(provider) => provider,
            None => {
                let msg = format!("no provider registered for sandbox class {sandbox_class}");
                tracing::warn!(%key, %msg);
                self.report_status(
                    &pod,
                    &key,
                    PodStatusUpdate {
                        reason: Some(REASON_UNSUPPORTED_CLASS.to_string()),
                        message: Some(msg),
                        sandbox_class: Some(sandbox_class.clone()),
                        provider: selected_provider.clone(),
                        ..PodStatusUpdate::new(PodPhase::Failed)
                    },
                )
                .await;
                return Ok(());
            }
        };
        let provider_name = provider.name().to_string();
        let provider_status = provider.capabilities().await.provider;
        if !provider_status.healthy {
            let msg = provider_status
                .reason
                .clone()
                .unwrap_or_else(|| format!("provider {provider_name} is unavailable"));
            self.report_status(
                &pod,
                &key,
                PodStatusUpdate {
                    reason: Some(REASON_PROVIDER_UNAVAILABLE.to_string()),
                    message: Some(msg),
                    sandbox_class: Some(sandbox_class.clone()),
                    provider: Some(provider_name.clone()),
                    ..PodStatusUpdate::new(PodPhase::Failed)
                },
            )
            .await;
            return Ok(());
        }
        if !provider_status.supports_sandbox_intent(&intent) {
            let msg = format!(
                "provider {provider_name} no longer supports selected sandbox intent: {}",
                intent.unsupported_message()
            );
            self.report_status(
                &pod,
                &key,
                PodStatusUpdate {
                    reason: Some(REASON_PROVIDER_UNAVAILABLE.to_string()),
                    message: Some(msg),
                    sandbox_class: Some(sandbox_class.clone()),
                    provider: Some(provider_name.clone()),
                    ..PodStatusUpdate::new(PodPhase::Failed)
                },
            )
            .await;
            return Ok(());
        }

        // Create sandbox if absent.
        let need_start;
        {
            let mut state = self.pods.entry(key.clone()).or_insert_with(|| PodState {
                uid: uid.clone(),
                ..Default::default()
            });
            state.uid = uid.clone();
            state.class = Some(class);
            state.sandbox_class = Some(sandbox_class.clone());
            state.provider = Some(provider_name.clone());
            if state.sandbox_id.is_none() {
                let spec = build_sandbox_spec(&pod, class, &sandbox_class, Some(&provider_name));
                match provider.create(spec).await {
                    Ok(id) => {
                        state.sandbox_id = Some(id);
                        need_start = true;
                    }
                    Err(e) => {
                        drop(state);
                        let msg = format!("create sandbox: {e}");
                        tracing::error!(%key, %msg);
                        self.report_status(
                            &pod,
                            &key,
                            PodStatusUpdate {
                                reason: Some(REASON_CREATE_FAILED.to_string()),
                                message: Some(msg),
                                sandbox_class: Some(sandbox_class.clone()),
                                provider: Some(provider_name.clone()),
                                ..PodStatusUpdate::new(PodPhase::Failed)
                            },
                        )
                        .await;
                        return Ok(());
                    }
                }
            } else {
                need_start = !state.started;
            }
        }

        let sandbox_id = self
            .pods
            .get(&key)
            .and_then(|s| s.sandbox_id.clone())
            .context("sandbox disappeared")?;

        if need_start {
            if let Err(e) = provider.start(&sandbox_id).await {
                let msg = format!("start sandbox: {e}");
                tracing::error!(%key, %msg);
                self.report_status(
                    &pod,
                    &key,
                    PodStatusUpdate {
                        reason: Some(REASON_START_FAILED.to_string()),
                        message: Some(msg),
                        sandbox_id: Some(sandbox_id.clone()),
                        sandbox_class: Some(sandbox_class.clone()),
                        provider: Some(provider_name.clone()),
                        ..PodStatusUpdate::new(PodPhase::Failed)
                    },
                )
                .await;
                return Ok(());
            }
            if let Some(mut state) = self.pods.get_mut(&key) {
                state.started = true;
            }
        }

        self.report_status(
            &pod,
            &key,
            PodStatusUpdate {
                sandbox_id: Some(sandbox_id.clone()),
                sandbox_class: Some(sandbox_class.clone()),
                provider: Some(provider_name.clone()),
                ..PodStatusUpdate::new(PodPhase::Running)
            },
        )
        .await;
        Ok(())
    }

    async fn sync_terminating(&self, pod: &Pod) -> Result<()> {
        let key = format!("{}/{}", pod.metadata.namespace, pod.metadata.name);
        self.teardown(&key).await;
        Ok(())
    }

    async fn pod_resync_loop(&self) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            tick.tick().await;
            if let Err(e) = self.pod_resync_once().await {
                tracing::warn!(error = %e, "pod resync failed");
            }
        }
    }

    async fn pod_resync_once(&self) -> Result<()> {
        let pods = self.client.list_pods().await?;
        let mut desired = std::collections::BTreeSet::new();
        for pod in pods {
            if pod.spec.node_name.as_deref() != Some(self.node_name.as_str()) {
                continue;
            }
            let key = format!("{}/{}", pod.metadata.namespace, pod.metadata.name);
            desired.insert(key.clone());
            if pod.metadata.deletion_timestamp.is_some() {
                self.teardown(&key).await;
            } else {
                self.sync_pod(pod).await?;
            }
        }

        let local_keys: Vec<String> = self.pods.iter().map(|entry| entry.key().clone()).collect();
        for key in local_keys {
            if !desired.contains(&key) {
                self.teardown(&key).await;
            }
        }
        Ok(())
    }

    async fn teardown(&self, key: &str) {
        let state = self.pods.remove(key);
        if let Some((_, state)) = state
            && let (Some(id), Some(class)) = (state.sandbox_id.as_ref(), state.class)
            && let Some(provider) = self.runtime.provider(class)
        {
            let _ = provider.stop(id, true).await;
            let _ = provider.remove(id).await;
        }
        tracing::info!(%key, "torendown pod");
    }

    // ---- Status reporting (CAS retry) ----

    async fn report_status(&self, pod: &Pod, key: &str, update: PodStatusUpdate) {
        // Suppress the feedback loop: skip if we already reported this phase.
        let already_reported = self
            .pods
            .get(key)
            .map(|s| s.reported_phase == Some(update.phase))
            .unwrap_or(false);
        if already_reported {
            return;
        }

        for _ in 0..5 {
            let mut latest = match self
                .client
                .get_pod(&pod.metadata.namespace, &pod.metadata.name)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error = %e, "status: get pod failed");
                    return;
                }
            };
            let started_at = boss_common::time::now_rfc3339();
            let status = latest.status.get_or_insert_default();
            status.phase = update.phase;
            status.message = update.message.clone();
            status.reason = update.reason.clone();
            status.sandbox_class = update.sandbox_class.clone();
            status.provider = update.provider.clone();
            if update.phase == PodPhase::Running {
                status.start_time = Some(started_at.clone());
                status.sandbox_id = update.sandbox_id.clone();
                status.host_ip = Some("127.0.0.1".into());
                let cs = latest
                    .spec
                    .containers
                    .first()
                    .map(|c| ContainerStatus {
                        name: c.name.clone(),
                        ready: true,
                        container_id: update.sandbox_id.clone(),
                        state: Some(ContainerState::Running { started_at }),
                    })
                    .unwrap_or_default();
                status.container_statuses = vec![cs];
            }
            match self
                .client
                .update_pod_status(&pod.metadata.namespace, &pod.metadata.name, &latest)
                .await
            {
                Ok(_) => {
                    if let Some(mut s) = self.pods.get_mut(key) {
                        s.reported_phase = Some(update.phase);
                    }
                    return;
                }
                Err(e) => {
                    tracing::debug!(error = %e, "status CAS retry");
                    continue;
                }
            }
        }
        tracing::warn!("status update exhausted retries");
    }

    // ---- PLEG: poll sandbox liveness, transition to Succeeded/Failed ----

    async fn pleg_loop(&self) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            tick.tick().await;
            if let Err(e) = self.pleg_once().await {
                tracing::warn!(error = %e, "pleg tick failed");
            }
        }
    }

    async fn pleg_once(&self) -> Result<()> {
        // Snapshot keys + sandbox ids + class hints.
        let entries: Vec<(String, String, Option<String>, Option<RuntimeClass>)> = self
            .pods
            .iter()
            .map(|e| {
                (
                    e.key().clone(),
                    e.uid.clone(),
                    e.sandbox_id.clone(),
                    e.class,
                )
            })
            .collect();

        for (key, uid, sandbox_id, class) in entries {
            let Some(sid) = sandbox_id else { continue };
            let Some(class) = class else { continue };
            let provider = match self.runtime.provider(class) {
                Some(p) => p,
                None => continue,
            };
            let status = match provider.status(&sid).await {
                Ok(s) => s,
                Err(_) => continue,
            };
            if status.running {
                continue;
            }
            // Exited: transition once.
            let already_terminated = self.pods.get(&key).map(|s| s.terminated).unwrap_or(true);
            if already_terminated {
                continue;
            }
            if let Some(mut s) = self.pods.get_mut(&key) {
                s.terminated = true;
            }
            let phase = if status.exit_code == Some(0) {
                PodPhase::Succeeded
            } else {
                PodPhase::Failed
            };
            let msg = status
                .exit_code
                .map(|c| format!("sandbox exited with code {c}"));
            // Build a minimal pod reference for status reporting.
            let (ns, name) = match key.split_once('/') {
                Some((ns, name)) => (ns.to_string(), name.to_string()),
                None => continue,
            };
            let pod = Pod {
                type_meta: TypeMeta::default(),
                metadata: ObjectMeta {
                    namespace: ns,
                    name,
                    uid: Some(uid),
                    ..Default::default()
                },
                spec: PodSpec::default(),
                status: None,
            };
            let (sandbox_class, provider) = self
                .pods
                .get(&key)
                .map(|s| (s.sandbox_class.clone(), s.provider.clone()))
                .unwrap_or_default();
            self.report_status(
                &pod,
                &key,
                PodStatusUpdate {
                    message: msg,
                    sandbox_id: Some(sid),
                    sandbox_class,
                    provider,
                    ..PodStatusUpdate::new(phase)
                },
            )
            .await;
        }
        Ok(())
    }
}

fn set_ready(status: &mut NodeStatus) {
    let now = boss_common::time::now_rfc3339();
    if let Some(c) = status.conditions.iter_mut().find(|c| c.kind == "Ready") {
        c.status = "True".into();
        c.last_heartbeat_time = Some(now);
    } else {
        status.conditions.push(NodeCondition {
            kind: "Ready".into(),
            status: "True".into(),
            last_heartbeat_time: Some(now),
            reason: Some("BossletReady".into()),
            message: Some("bosslet is posting ready status".into()),
        });
    }
}

fn runtime_class_for_sandbox(class: &str) -> Option<RuntimeClass> {
    match normalize_sandbox_class(class)? {
        "process" => Some(RuntimeClass::BareMetal),
        "container" => Some(RuntimeClass::Container),
        "wasm" => Some(RuntimeClass::Wasm),
        "vm" | "microvm" => Some(RuntimeClass::Vm),
        _ => None,
    }
}

fn selected_provider_for_pod(pod: &Pod) -> Option<String> {
    pod.metadata
        .annotations
        .as_ref()
        .and_then(|annotations| annotations.get(ANNOTATION_SELECTED_PROVIDER))
        .cloned()
}

fn build_sandbox_spec(
    pod: &Pod,
    class: RuntimeClass,
    sandbox_class: &str,
    provider: Option<&str>,
) -> SandboxSpec {
    let c = pod.spec.containers.first();
    let artifact = pod
        .spec
        .sandbox
        .as_ref()
        .and_then(|sandbox| sandbox.artifact.as_ref());
    let mut command = c.map(|c| c.command.clone()).unwrap_or_default();
    if command.is_empty()
        && sandbox_class == "process"
        && let Some(path) = artifact.and_then(|artifact| artifact.path.clone())
    {
        command.push(path);
    }
    let args = c.map(|c| c.args.clone()).unwrap_or_default();
    let env = c
        .map(|c| {
            c.env
                .iter()
                .map(|e| (e.name.clone(), e.value.clone()))
                .collect()
        })
        .unwrap_or_default();
    SandboxSpec {
        pod_uid: pod.metadata.uid.clone().unwrap_or_default(),
        pod_name: pod.metadata.name.clone(),
        namespace: pod.metadata.namespace.clone(),
        runtime_class: class,
        sandbox_class: sandbox_class.to_string(),
        provider: provider.map(str::to_string),
        command,
        args,
        env,
        image: c
            .and_then(|c| c.image.clone())
            .or_else(|| artifact.and_then(|artifact| artifact.image.clone()))
            .or_else(|| artifact.and_then(|artifact| artifact.uri.clone())),
        artifact_type: artifact.map(|artifact| artifact.kind.clone()),
        artifact_uri: c
            .and_then(|c| c.wasm_module.clone())
            .or_else(|| artifact.and_then(|artifact| artifact.uri.clone())),
        artifact_path: artifact.and_then(|artifact| artifact.path.clone()),
        wasm_module: None,
        network: network_mode_for_pod(pod),
    }
}

fn network_mode_for_pod(pod: &Pod) -> NetworkMode {
    pod.spec
        .sandbox
        .as_ref()
        .and_then(|sandbox| sandbox.network.as_ref())
        .and_then(|network| network.mode.as_deref())
        .map(|mode| match mode.to_ascii_lowercase().as_str() {
            "none" => NetworkMode::None,
            _ => NetworkMode::Host,
        })
        .unwrap_or(NetworkMode::Host)
}

// Re-export Object for completeness.
#[allow(unused)]
type _PodObject = Object<PodSpec>;
