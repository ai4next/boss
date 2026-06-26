//! Scheduler: watches pending pods, filters nodes by runtime capability, and
//! binds each pod to a node/provider pair.

use async_trait::async_trait;
use boss_api::{
    ANNOTATION_RESOLVED_SANDBOX_CLASS, ANNOTATION_SELECTED_PROVIDER, Node, Pod, PodPhase,
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
        let class = pod.spec.resolved_sandbox_class();
        let nodes: Vec<Node> = self.storage.list(&build_prefix("nodes", None)).await?;
        let mut candidates: Vec<(String, String)> = nodes
            .iter()
            .filter_map(|node| {
                matching_provider(node, &class)
                    .map(|provider| (node.metadata.name.clone(), provider.name.clone()))
            })
            .collect();
        candidates.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

        let Some((node_name, provider_name)) = candidates.into_iter().next() else {
            tracing::debug!(
                pod = %pod.metadata.name,
                namespace = %pod.metadata.namespace,
                sandbox_class = %class,
                "no matching node found"
            );
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
            annotations.insert(ANNOTATION_RESOLVED_SANDBOX_CLASS.to_string(), class.clone());
            annotations.insert(
                ANNOTATION_SELECTED_PROVIDER.to_string(),
                provider_name.clone(),
            );
            match self.storage.update::<Pod>(&key, &latest).await {
                Ok(_) => {
                    tracing::info!(
                        pod = %pod.metadata.name,
                        namespace = %pod.metadata.namespace,
                        node = %node_name,
                        provider = %provider_name,
                        sandbox_class = %class,
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
    class: &str,
) -> Option<&'a boss_api::RuntimeProviderStatus> {
    if node.spec.unschedulable || !node_ready(node) {
        return None;
    }
    node.status
        .as_ref()
        .and_then(|status| status.runtime_capabilities.as_ref())
        .and_then(|capabilities| capabilities.healthy_provider_for_class(class))
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
