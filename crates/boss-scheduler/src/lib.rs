//! Scheduler: watches pending pods, filters nodes by runtime capability, and
//! binds each pod to a node/provider pair.

use async_trait::async_trait;
use boss_api::{
    ANNOTATION_RESOLVED_SANDBOX_CLASS, ANNOTATION_SELECTED_PROVIDER, Node, Pod, PodPhase,
    REASON_UNSCHEDULABLE, ResolvedSandboxIntent,
};
use boss_store::{Storage, StorageBackend, build_key, build_prefix};
use std::sync::Arc;

/// A filter plugin rejects a pod-node pair.
#[async_trait]
pub trait FilterPlugin: Send + Sync {
    fn name(&self) -> &'static str;
    async fn filter(&self, pod: &Pod, node: &Node) -> Result<(), String>;
}

/// A score plugin ranks a pod-node pair; higher is better.
#[async_trait]
pub trait ScorePlugin: Send + Sync {
    fn name(&self) -> &'static str;
    async fn score(&self, pod: &Pod, node: &Node) -> i64;
}

/// Scheduler runs the pipeline over pending pods and binds them.
#[allow(unused_variables)]
pub struct Scheduler {
    pub storage: Arc<StorageBackend>,
}

impl Scheduler {
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self { storage }
    }

    pub async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        tracing::info!("scheduler started");
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            tick.tick().await;
            if let Err(error) = self.schedule_once().await {
                tracing::warn!(%error, "scheduler tick failed");
            }
        }
    }

    async fn schedule_once(&self) -> anyhow::Result<()> {
        let pods: Vec<Pod> = self.storage.list(&build_prefix("pods", None)).await?;
        for pod in pods {
            if !is_pending_unbound(&pod) {
                continue;
            }
            if let Err(error) = self.schedule_pod(pod).await {
                tracing::warn!(%error, "pod scheduling failed");
            }
        }
        Ok(())
    }

    async fn schedule_pod(&self, pod: Pod) -> anyhow::Result<()> {
        let intent = match pod.spec.try_resolved_sandbox_intent() {
            Ok(intent) => intent,
            Err(error) => {
                self.mark_pending(&pod, error.reason, &error.message, None, None)
                    .await?;
                return Ok(());
            }
        };
        let nodes: Vec<Node> = self.storage.list(&build_prefix("nodes", None)).await?;
        let mut candidates: Vec<(String, String)> = nodes
            .iter()
            .filter(|node| node_matches_pod_constraints(&pod, node))
            .filter_map(|node| {
                matching_provider(node, &intent)
                    .map(|provider| (node.metadata.name.clone(), provider.name.clone()))
            })
            .collect();
        candidates.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

        let Some((node_name, provider_name)) = candidates.into_iter().next() else {
            tracing::debug!(
                pod = %pod.metadata.name,
                namespace = %pod.metadata.namespace,
                sandbox_class = %intent.class,
                "no matching node found"
            );
            self.mark_pending(
                &pod,
                REASON_UNSCHEDULABLE,
                &intent.unsupported_message(),
                Some(&intent.class),
                None,
            )
            .await?;
            return Ok(());
        };

        for _ in 0..5 {
            let key = build_key("pods", Some(&pod.metadata.namespace), &pod.metadata.name);
            let mut latest: Pod = self.storage.get(&key).await?;
            if !is_pending_unbound(&latest) {
                return Ok(());
            }
            latest.spec.node_name = Some(node_name.clone());
            let annotations = latest
                .metadata
                .annotations
                .get_or_insert_with(Default::default);
            annotations.insert(
                ANNOTATION_RESOLVED_SANDBOX_CLASS.to_string(),
                intent.class.clone(),
            );
            annotations.insert(
                ANNOTATION_SELECTED_PROVIDER.to_string(),
                provider_name.clone(),
            );
            if let Some(status) = latest.status.as_mut() {
                status.phase = PodPhase::Pending;
                status.reason = None;
                status.message = None;
                status.sandbox_class = Some(intent.class.clone());
                status.provider = Some(provider_name.clone());
            }
            match self.storage.update::<Pod>(&key, &latest).await {
                Ok(_) => {
                    tracing::info!(
                        pod = %pod.metadata.name,
                        namespace = %pod.metadata.namespace,
                        node = %node_name,
                        provider = %provider_name,
                        sandbox_class = %intent.class,
                        "bound pod"
                    );
                    return Ok(());
                }
                Err(error) => {
                    tracing::debug!(%error, "bind CAS retry");
                    continue;
                }
            }
        }
        Err(anyhow::anyhow!("bind retries exhausted"))
    }

    async fn mark_pending(
        &self,
        pod: &Pod,
        reason: &str,
        message: &str,
        sandbox_class: Option<&str>,
        provider: Option<&str>,
    ) -> anyhow::Result<()> {
        for _ in 0..5 {
            let key = build_key("pods", Some(&pod.metadata.namespace), &pod.metadata.name);
            let mut latest: Pod = self.storage.get(&key).await?;
            if !is_pending_unbound(&latest) {
                return Ok(());
            }
            let status = latest.status.get_or_insert_default();
            if status.phase == PodPhase::Pending
                && status.reason.as_deref() == Some(reason)
                && status.message.as_deref() == Some(message)
                && status.sandbox_class.as_deref() == sandbox_class
                && status.provider.as_deref() == provider
            {
                return Ok(());
            }
            status.phase = PodPhase::Pending;
            status.reason = Some(reason.to_string());
            status.message = Some(message.to_string());
            status.sandbox_class = sandbox_class.map(str::to_string);
            status.provider = provider.map(str::to_string);
            match self.storage.update::<Pod>(&key, &latest).await {
                Ok(_) => return Ok(()),
                Err(error) => {
                    tracing::debug!(%error, "pending status CAS retry");
                    continue;
                }
            }
        }
        Err(anyhow::anyhow!("pending status retries exhausted"))
    }
}

fn is_pending_unbound(pod: &Pod) -> bool {
    if pod.spec.node_name.is_some() || pod.metadata.deletion_timestamp.is_some() {
        return false;
    }
    pod.status
        .as_ref()
        .map(|status| matches!(status.phase, PodPhase::Pending | PodPhase::Unknown))
        .unwrap_or(true)
}

fn matching_provider<'a>(
    node: &'a Node,
    intent: &ResolvedSandboxIntent,
) -> Option<&'a boss_api::RuntimeProviderStatus> {
    if node.spec.unschedulable || !node_ready(node) {
        return None;
    }
    node.status
        .as_ref()
        .and_then(|status| status.runtime_capabilities.as_ref())
        .and_then(|capabilities| capabilities.healthy_provider_for_intent(intent))
}

fn node_matches_pod_constraints(pod: &Pod, node: &Node) -> bool {
    node_selector_matches(pod, node) && taints_tolerated(pod, node)
}

fn node_selector_matches(pod: &Pod, node: &Node) -> bool {
    let Some(selector) = &pod.spec.node_selector else {
        return true;
    };
    let labels = node.metadata.labels.as_ref().or(node.spec.labels.as_ref());
    selector
        .iter()
        .all(|(key, value)| labels.and_then(|labels| labels.get(key)) == Some(value))
}

fn taints_tolerated(pod: &Pod, node: &Node) -> bool {
    node.spec.taints.iter().all(|taint| {
        pod.spec.tolerations.iter().any(|toleration| {
            toleration.key == taint.key
                && toleration.effect == taint.effect
                && (toleration.operator == "Exists"
                    || (toleration.operator == "Equal" && toleration.value == taint.value))
        })
    })
}

fn node_ready(node: &Node) -> bool {
    node.status
        .as_ref()
        .map(|status| {
            status
                .conditions
                .iter()
                .any(|condition| condition.kind == "Ready" && condition.status == "True")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use boss_api::{
        Labels, NodeCondition, NodeSpec, NodeStatus, Object, ObjectMeta, PodSpec, Resource,
        RuntimeCapabilities, RuntimeProviderStatus, SandboxArtifact, SandboxNetwork,
        SandboxRequirements, Taint, Toleration, TypeMeta,
    };
    use boss_store::MemoryStorage;

    fn ready_node(name: &str, provider: RuntimeProviderStatus) -> Node {
        Object {
            type_meta: TypeMeta {
                api_version: NodeSpec::API_VERSION.to_string(),
                kind: NodeSpec::KIND.to_string(),
            },
            metadata: ObjectMeta {
                name: name.to_string(),
                ..Default::default()
            },
            spec: NodeSpec::default(),
            status: Some(NodeStatus {
                conditions: vec![NodeCondition {
                    kind: "Ready".to_string(),
                    status: "True".to_string(),
                    ..Default::default()
                }],
                runtime_capabilities: Some(RuntimeCapabilities {
                    providers: vec![provider],
                }),
                ..Default::default()
            }),
        }
    }

    fn process_provider() -> RuntimeProviderStatus {
        provider(
            "baremetal",
            vec!["process"],
            vec!["executable"],
            vec!["host"],
            vec!["sharedHost"],
        )
    }

    fn provider(
        name: &str,
        classes: Vec<&str>,
        artifact_types: Vec<&str>,
        network_modes: Vec<&str>,
        isolation_levels: Vec<&str>,
    ) -> RuntimeProviderStatus {
        RuntimeProviderStatus {
            name: name.to_string(),
            healthy: true,
            classes: classes.into_iter().map(str::to_string).collect(),
            artifact_types: artifact_types.into_iter().map(str::to_string).collect(),
            network_modes: network_modes.into_iter().map(str::to_string).collect(),
            isolation_levels: isolation_levels.into_iter().map(str::to_string).collect(),
            ..Default::default()
        }
    }

    fn pod(name: &str, sandbox_class: &str, artifact_type: &str, network_mode: &str) -> Pod {
        Object {
            type_meta: TypeMeta {
                api_version: PodSpec::API_VERSION.to_string(),
                kind: PodSpec::KIND.to_string(),
            },
            metadata: ObjectMeta {
                name: name.to_string(),
                namespace: "default".to_string(),
                ..Default::default()
            },
            spec: PodSpec {
                sandbox_class: Some(sandbox_class.to_string()),
                sandbox: Some(SandboxRequirements {
                    artifact: Some(SandboxArtifact {
                        kind: artifact_type.to_string(),
                        ..Default::default()
                    }),
                    network: Some(SandboxNetwork {
                        mode: Some(network_mode.to_string()),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            status: None,
        }
    }

    #[tokio::test]
    async fn scheduler_binds_to_provider_matching_full_intent() {
        let storage = Arc::new(StorageBackend::Memory(MemoryStorage::arc()));
        let scheduler = Scheduler::new(storage.clone());
        let node = ready_node("node-a", process_provider());
        storage
            .create(&build_key("nodes", None, "node-a"), &node)
            .await
            .unwrap();
        let pod = pod("sleep", "process", "executable", "host");
        storage
            .create(&build_key("pods", Some("default"), "sleep"), &pod)
            .await
            .unwrap();

        scheduler.schedule_once().await.unwrap();

        let scheduled: Pod = storage
            .get(&build_key("pods", Some("default"), "sleep"))
            .await
            .unwrap();
        assert_eq!(scheduled.spec.node_name.as_deref(), Some("node-a"));
        assert_eq!(
            scheduled
                .metadata
                .annotations
                .as_ref()
                .and_then(|annotations| annotations.get(ANNOTATION_SELECTED_PROVIDER))
                .map(String::as_str),
            Some("baremetal")
        );
    }

    #[tokio::test]
    async fn scheduler_marks_pod_pending_when_intent_is_unsupported() {
        let storage = Arc::new(StorageBackend::Memory(MemoryStorage::arc()));
        let scheduler = Scheduler::new(storage.clone());
        let node = ready_node("node-a", process_provider());
        storage
            .create(&build_key("nodes", None, "node-a"), &node)
            .await
            .unwrap();
        let pod = pod("wasm", "wasm", "wasmModule", "none");
        storage
            .create(&build_key("pods", Some("default"), "wasm"), &pod)
            .await
            .unwrap();

        scheduler.schedule_once().await.unwrap();

        let unscheduled: Pod = storage
            .get(&build_key("pods", Some("default"), "wasm"))
            .await
            .unwrap();
        assert_eq!(unscheduled.spec.node_name, None);
        let status = unscheduled.status.unwrap();
        assert_eq!(status.phase, PodPhase::Pending);
        assert_eq!(status.reason.as_deref(), Some(REASON_UNSCHEDULABLE));
    }

    #[tokio::test]
    async fn scheduler_honors_node_selector_and_taints() {
        let storage = Arc::new(StorageBackend::Memory(MemoryStorage::arc()));
        let scheduler = Scheduler::new(storage.clone());

        let mut wrong = ready_node("node-a", process_provider());
        wrong.metadata.labels = Some(Labels::from([("disk".to_string(), "hdd".to_string())]));
        storage
            .create(&build_key("nodes", None, "node-a"), &wrong)
            .await
            .unwrap();

        let mut right = ready_node("node-b", process_provider());
        right.metadata.labels = Some(Labels::from([("disk".to_string(), "ssd".to_string())]));
        right.spec.taints = vec![Taint {
            key: "dedicated".to_string(),
            value: "sandbox".to_string(),
            effect: "NoSchedule".to_string(),
        }];
        storage
            .create(&build_key("nodes", None, "node-b"), &right)
            .await
            .unwrap();

        let mut pod = pod("sleep", "process", "executable", "host");
        pod.spec.node_selector = Some(Labels::from([("disk".to_string(), "ssd".to_string())]));
        pod.spec.tolerations = vec![Toleration {
            key: "dedicated".to_string(),
            operator: "Equal".to_string(),
            value: "sandbox".to_string(),
            effect: "NoSchedule".to_string(),
        }];
        storage
            .create(&build_key("pods", Some("default"), "sleep"), &pod)
            .await
            .unwrap();

        scheduler.schedule_once().await.unwrap();

        let scheduled: Pod = storage
            .get(&build_key("pods", Some("default"), "sleep"))
            .await
            .unwrap();
        assert_eq!(scheduled.spec.node_name.as_deref(), Some("node-b"));
    }
}
