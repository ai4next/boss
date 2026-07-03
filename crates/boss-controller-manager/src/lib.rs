//! Controller manager. Reconciles declarative app resources into lower-level
//! objects using idempotent storage operations and optimistic concurrency.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use boss_api::{
    Condition, Deployment, DeploymentStatus, Object, ObjectMeta, OwnerReference, Pod, PodPhase,
    PodSpec, ReplicaSet, ReplicaSetSpec, ReplicaSetStatus, Resource, ResourceVersion, TypeMeta,
};
use boss_store::error::StoreError;
use boss_store::{Storage, StorageBackend, WatchEvent, build_key, build_prefix};
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use tokio_stream::StreamExt;

/// Result of a single reconcile pass.
#[derive(Clone, Debug, Default)]
pub struct ReconcileResult {
    pub requeue: bool,
}

/// A reconciler drives one resource kind toward desired state.
#[async_trait]
pub trait Reconciler: Send + Sync + 'static {
    async fn reconcile(&self, key: &str) -> anyhow::Result<ReconcileResult>;
}

/// Controller manager owns all controllers.
pub struct ControllerManager {
    pub storage: Arc<StorageBackend>,
}

impl ControllerManager {
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self { storage }
    }

    pub async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        tracing::info!("controller-manager started");
        let deployment_controller = Arc::new(DeploymentController::new(self.storage.clone()));
        let replicaset_controller = Arc::new(ReplicaSetController::new(self.storage.clone()));
        let garbage_collector = Arc::new(GarbageCollector::new(self.storage.clone()));

        let (deployment_queue, deployment_rx) = WorkQueue::new("deployments");
        let (replicaset_queue, replicaset_rx) = WorkQueue::new("replicasets");
        let (gc_queue, gc_rx) = WorkQueue::new("garbage-collector");

        seed_queues(
            &self.storage,
            &deployment_queue,
            &replicaset_queue,
            &gc_queue,
        )
        .await?;

        tokio::spawn(run_reconciler_worker(
            "deployment",
            deployment_controller,
            deployment_queue.clone(),
            deployment_rx,
        ));
        tokio::spawn(run_reconciler_worker(
            "replicaset",
            replicaset_controller,
            replicaset_queue.clone(),
            replicaset_rx,
        ));
        tokio::spawn(run_gc_worker(garbage_collector, gc_queue.clone(), gc_rx));
        tokio::spawn(watch_deployments(
            self.storage.clone(),
            deployment_queue.clone(),
            gc_queue.clone(),
        ));
        tokio::spawn(watch_replica_sets(
            self.storage.clone(),
            deployment_queue.clone(),
            replicaset_queue.clone(),
            gc_queue.clone(),
        ));
        tokio::spawn(watch_pods(
            self.storage.clone(),
            replicaset_queue.clone(),
            gc_queue.clone(),
        ));
        tokio::spawn(periodic_resync(
            self.storage.clone(),
            deployment_queue,
            replicaset_queue,
            gc_queue,
        ));

        std::future::pending::<anyhow::Result<()>>().await
    }
}

#[derive(Clone)]
struct WorkQueue {
    name: &'static str,
    tx: mpsc::UnboundedSender<String>,
    queued: Arc<std::sync::Mutex<BTreeSet<String>>>,
}

impl WorkQueue {
    fn new(name: &'static str) -> (Self, mpsc::UnboundedReceiver<String>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                name,
                tx,
                queued: Arc::new(std::sync::Mutex::new(BTreeSet::new())),
            },
            rx,
        )
    }

    fn enqueue(&self, key: impl Into<String>) {
        let key = key.into();
        let inserted = {
            let mut queued = self.queued.lock().expect("workqueue mutex poisoned");
            queued.insert(key.clone())
        };
        if inserted && self.tx.send(key.clone()).is_err() {
            self.queued
                .lock()
                .expect("workqueue mutex poisoned")
                .remove(&key);
        }
    }

    fn start_processing(&self, key: &str) {
        self.queued
            .lock()
            .expect("workqueue mutex poisoned")
            .remove(key);
    }

    fn requeue_after(&self, key: String, delay: Duration) {
        let queue = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            queue.enqueue(key);
        });
    }
}

async fn run_reconciler_worker<R>(
    controller_name: &'static str,
    controller: Arc<R>,
    queue: WorkQueue,
    mut rx: mpsc::UnboundedReceiver<String>,
) where
    R: Reconciler,
{
    while let Some(key) = rx.recv().await {
        queue.start_processing(&key);
        match controller.reconcile(&key).await {
            Ok(result) => {
                if result.requeue {
                    queue.requeue_after(key, Duration::from_secs(1));
                }
            }
            Err(error) => {
                tracing::warn!(%error, %key, controller = controller_name, "reconcile failed");
                queue.requeue_after(key, Duration::from_secs(2));
            }
        }
    }
    tracing::warn!(queue = queue.name, "workqueue closed");
}

async fn run_gc_worker(
    garbage_collector: Arc<GarbageCollector>,
    queue: WorkQueue,
    mut rx: mpsc::UnboundedReceiver<String>,
) {
    while let Some(key) = rx.recv().await {
        queue.start_processing(&key);
        if let Err(error) = garbage_collector.reconcile_all().await {
            tracing::warn!(%error, trigger = %key, "garbage collection failed");
            queue.requeue_after(key, Duration::from_secs(2));
        }
    }
    tracing::warn!(queue = queue.name, "workqueue closed");
}

async fn seed_queues(
    storage: &StorageBackend,
    deployment_queue: &WorkQueue,
    replicaset_queue: &WorkQueue,
    gc_queue: &WorkQueue,
) -> anyhow::Result<()> {
    let deployments: Vec<Deployment> = storage.list(&build_prefix("deployments", None)).await?;
    for deployment in deployments {
        deployment_queue.enqueue(object_key("deployments", &deployment));
    }

    let replica_sets: Vec<ReplicaSet> = storage.list(&build_prefix("replicasets", None)).await?;
    for replica_set in replica_sets {
        replicaset_queue.enqueue(object_key("replicasets", &replica_set));
        enqueue_owner_deployment(&replica_set, deployment_queue);
    }

    let pods: Vec<Pod> = storage.list(&build_prefix("pods", None)).await?;
    for pod in pods {
        enqueue_owner_replicaset(&pod, replicaset_queue);
    }

    gc_queue.enqueue("gc/resync");
    Ok(())
}

async fn periodic_resync(
    storage: Arc<StorageBackend>,
    deployment_queue: WorkQueue,
    replicaset_queue: WorkQueue,
    gc_queue: WorkQueue,
) {
    let mut tick = interval(Duration::from_secs(30));
    loop {
        tick.tick().await;
        if let Err(error) =
            seed_queues(&storage, &deployment_queue, &replicaset_queue, &gc_queue).await
        {
            tracing::warn!(%error, "controller resync failed");
        }
    }
}

async fn watch_deployments(
    storage: Arc<StorageBackend>,
    deployment_queue: WorkQueue,
    gc_queue: WorkQueue,
) {
    let mut resource_version = ResourceVersion(0);
    loop {
        match storage
            .watch(&build_prefix("deployments", None), resource_version)
            .await
        {
            Ok(mut stream) => {
                while let Some(event) = stream.next().await {
                    resource_version = event_resource_version(&event).unwrap_or(resource_version);
                    if let Some(deployment) = event_object::<Deployment>(&event) {
                        deployment_queue.enqueue(object_key("deployments", &deployment));
                    }
                    if matches!(event, WatchEvent::Deleted(_, _)) {
                        gc_queue.enqueue("gc/deployment-delete");
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "deployment watch failed"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn watch_replica_sets(
    storage: Arc<StorageBackend>,
    deployment_queue: WorkQueue,
    replicaset_queue: WorkQueue,
    gc_queue: WorkQueue,
) {
    let mut resource_version = ResourceVersion(0);
    loop {
        match storage
            .watch(&build_prefix("replicasets", None), resource_version)
            .await
        {
            Ok(mut stream) => {
                while let Some(event) = stream.next().await {
                    resource_version = event_resource_version(&event).unwrap_or(resource_version);
                    if let Some(replica_set) = event_object::<ReplicaSet>(&event) {
                        replicaset_queue.enqueue(object_key("replicasets", &replica_set));
                        enqueue_owner_deployment(&replica_set, &deployment_queue);
                    }
                    if matches!(event, WatchEvent::Deleted(_, _)) {
                        gc_queue.enqueue("gc/replicaset-delete");
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "replicaset watch failed"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn watch_pods(
    storage: Arc<StorageBackend>,
    replicaset_queue: WorkQueue,
    gc_queue: WorkQueue,
) {
    let mut resource_version = ResourceVersion(0);
    loop {
        match storage
            .watch(&build_prefix("pods", None), resource_version)
            .await
        {
            Ok(mut stream) => {
                while let Some(event) = stream.next().await {
                    resource_version = event_resource_version(&event).unwrap_or(resource_version);
                    if let Some(pod) = event_object::<Pod>(&event) {
                        enqueue_owner_replicaset(&pod, &replicaset_queue);
                    }
                    if matches!(event, WatchEvent::Deleted(_, _)) {
                        gc_queue.enqueue("gc/pod-delete");
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "pod watch failed"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn object_key<T: Resource>(resource: &str, object: &Object<T>) -> String {
    build_key(
        resource,
        Some(&object.metadata.namespace),
        &object.metadata.name,
    )
}

fn enqueue_owner_deployment(replica_set: &ReplicaSet, queue: &WorkQueue) {
    let Some(owner) = controller_owner(&replica_set.metadata) else {
        return;
    };
    if owner.kind == "Deployment" {
        queue.enqueue(build_key(
            "deployments",
            Some(&replica_set.metadata.namespace),
            &owner.name,
        ));
    }
}

fn enqueue_owner_replicaset(pod: &Pod, queue: &WorkQueue) {
    let Some(owner) = controller_owner(&pod.metadata) else {
        return;
    };
    if owner.kind == "ReplicaSet" {
        queue.enqueue(build_key(
            "replicasets",
            Some(&pod.metadata.namespace),
            &owner.name,
        ));
    }
}

fn event_object<T: serde::de::DeserializeOwned>(event: &WatchEvent) -> Option<T> {
    serde_json::from_value(event.object().clone()).ok()
}

fn event_resource_version(event: &WatchEvent) -> Option<ResourceVersion> {
    event
        .object()
        .get("metadata")
        .and_then(|metadata| metadata.get("resourceVersion"))
        .and_then(|value| value.as_u64())
        .map(ResourceVersion)
}

struct DeploymentController {
    storage: Arc<StorageBackend>,
}

impl DeploymentController {
    fn new(storage: Arc<StorageBackend>) -> Self {
        Self { storage }
    }

    async fn reconcile_deleted(&self, deployment: &Deployment) -> anyhow::Result<()> {
        let replica_sets = owned_replica_sets(&self.storage, deployment).await?;
        for replica_set in replica_sets {
            delete_if_exists(
                &self.storage,
                "replicasets",
                Some(&replica_set.metadata.namespace),
                &replica_set.metadata.name,
            )
            .await?;
        }
        Ok(())
    }

    async fn reconcile_active(&self, deployment: &Deployment) -> anyhow::Result<DeploymentStatus> {
        validate_selector_matches_template(
            &deployment.spec.selector,
            deployment.spec.template.metadata.labels.as_ref(),
        )?;
        let replica_set_name = current_replica_set_name(deployment);
        let replica_set_key = build_key(
            "replicasets",
            Some(&deployment.metadata.namespace),
            &replica_set_name,
        );
        let desired = replica_set_for_deployment(deployment, &replica_set_name);
        match self.storage.get::<ReplicaSet>(&replica_set_key).await {
            Ok(mut existing) => {
                let mut changed = false;
                if existing.spec.replicas != deployment.spec.replicas {
                    existing.spec.replicas = deployment.spec.replicas;
                    changed = true;
                }
                if serde_json::to_value(&existing.spec.selector)?
                    != serde_json::to_value(&desired.spec.selector)?
                {
                    existing.spec.selector = desired.spec.selector.clone();
                    changed = true;
                }
                if serde_json::to_value(&existing.spec.template)?
                    != serde_json::to_value(&desired.spec.template)?
                {
                    existing.spec.template = desired.spec.template.clone();
                    changed = true;
                }
                if changed {
                    self.storage
                        .update::<ReplicaSet>(&replica_set_key, &existing)
                        .await?;
                }
            }
            Err(StoreError::NotFound(_)) => {
                self.storage
                    .create::<ReplicaSet>(&replica_set_key, &desired)
                    .await?;
            }
            Err(error) => return Err(error.into()),
        }

        let replica_sets = owned_replica_sets(&self.storage, deployment).await?;
        let mut status = DeploymentStatus {
            observed_generation: deployment.metadata.generation,
            ..Default::default()
        };
        for replica_set in replica_sets {
            let rs_status = replica_set.status.unwrap_or_default();
            status.replicas += rs_status.replicas;
            status.ready_replicas += rs_status.ready_replicas;
            status.available_replicas += rs_status.available_replicas;
            if replica_set.metadata.name == replica_set_name {
                status.updated_replicas += rs_status.replicas;
            }
        }
        status.conditions = deployment_conditions(
            deployment,
            &status,
            false,
            "ReconcileComplete",
            "desired ReplicaSet is reconciled",
        );
        Ok(status)
    }
}

#[async_trait]
impl Reconciler for DeploymentController {
    async fn reconcile(&self, key: &str) -> anyhow::Result<ReconcileResult> {
        let mut deployment: Deployment = match self.storage.get(key).await {
            Ok(deployment) => deployment,
            Err(StoreError::NotFound(_)) => return Ok(ReconcileResult::default()),
            Err(error) => return Err(error.into()),
        };

        if deployment.metadata.deletion_timestamp.is_some() {
            self.reconcile_deleted(&deployment).await?;
            return Ok(ReconcileResult::default());
        }

        let status = match self.reconcile_active(&deployment).await {
            Ok(status) => status,
            Err(error) => DeploymentStatus {
                observed_generation: deployment.metadata.generation,
                conditions: vec![Condition::new(
                    "Degraded",
                    "True",
                    "ReconcileFailed",
                    error.to_string(),
                    deployment.metadata.generation,
                )],
                ..deployment.status.clone().unwrap_or_default()
            },
        };
        if deployment
            .status
            .as_ref()
            .is_some_and(|existing| deployment_status_semantically_equal(existing, &status))
        {
            return Ok(ReconcileResult::default());
        }
        deployment.status = Some(status);
        update_status(&self.storage, key, deployment).await?;
        Ok(ReconcileResult::default())
    }
}

struct ReplicaSetController {
    storage: Arc<StorageBackend>,
}

impl ReplicaSetController {
    fn new(storage: Arc<StorageBackend>) -> Self {
        Self { storage }
    }

    async fn reconcile_deleted(&self, replica_set: &ReplicaSet) -> anyhow::Result<()> {
        let pods = owned_pods(&self.storage, replica_set).await?;
        for pod in pods {
            delete_if_exists(
                &self.storage,
                "pods",
                Some(&pod.metadata.namespace),
                &pod.metadata.name,
            )
            .await?;
        }
        Ok(())
    }

    async fn reconcile_active(&self, replica_set: &ReplicaSet) -> anyhow::Result<ReplicaSetStatus> {
        validate_selector_matches_template(
            &replica_set.spec.selector,
            replica_set.spec.template.metadata.labels.as_ref(),
        )?;
        let mut pods = owned_pods(&self.storage, replica_set).await?;
        pods.sort_by(|left, right| left.metadata.name.cmp(&right.metadata.name));
        let desired = replica_set.spec.replicas.max(0) as usize;
        if pods.len() < desired {
            for index in pods.len()..desired {
                let pod = pod_for_replica_set(replica_set, index);
                let key = build_key("pods", Some(&pod.metadata.namespace), &pod.metadata.name);
                match self.storage.create::<Pod>(&key, &pod).await {
                    Ok(_) => {}
                    Err(StoreError::AlreadyExists(_)) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        } else if pods.len() > desired {
            for pod in pods.iter().skip(desired) {
                delete_if_exists(
                    &self.storage,
                    "pods",
                    Some(&pod.metadata.namespace),
                    &pod.metadata.name,
                )
                .await?;
            }
        }

        let pods = owned_pods(&self.storage, replica_set).await?;
        let mut status = ReplicaSetStatus {
            observed_generation: replica_set.metadata.generation,
            replicas: pods.len() as i32,
            ..Default::default()
        };
        for pod in pods {
            let phase = pod
                .status
                .as_ref()
                .map(|status| status.phase)
                .unwrap_or_default();
            if matches!(phase, PodPhase::Running | PodPhase::Succeeded) {
                status.ready_replicas += 1;
                status.available_replicas += 1;
            }
        }
        status.conditions = replicaset_conditions(
            replica_set,
            &status,
            false,
            "ReconcileComplete",
            "desired Pods are reconciled",
        );
        Ok(status)
    }
}

#[async_trait]
impl Reconciler for ReplicaSetController {
    async fn reconcile(&self, key: &str) -> anyhow::Result<ReconcileResult> {
        let mut replica_set: ReplicaSet = match self.storage.get(key).await {
            Ok(replica_set) => replica_set,
            Err(StoreError::NotFound(_)) => return Ok(ReconcileResult::default()),
            Err(error) => return Err(error.into()),
        };

        if replica_set.metadata.deletion_timestamp.is_some() {
            self.reconcile_deleted(&replica_set).await?;
            return Ok(ReconcileResult::default());
        }

        let status = match self.reconcile_active(&replica_set).await {
            Ok(status) => status,
            Err(error) => ReplicaSetStatus {
                observed_generation: replica_set.metadata.generation,
                conditions: vec![Condition::new(
                    "Degraded",
                    "True",
                    "ReconcileFailed",
                    error.to_string(),
                    replica_set.metadata.generation,
                )],
                ..replica_set.status.clone().unwrap_or_default()
            },
        };
        if replica_set
            .status
            .as_ref()
            .is_some_and(|existing| replicaset_status_semantically_equal(existing, &status))
        {
            return Ok(ReconcileResult::default());
        }
        replica_set.status = Some(status);
        update_status(&self.storage, key, replica_set).await?;
        Ok(ReconcileResult::default())
    }
}

struct GarbageCollector {
    storage: Arc<StorageBackend>,
}

impl GarbageCollector {
    fn new(storage: Arc<StorageBackend>) -> Self {
        Self { storage }
    }

    async fn reconcile_all(&self) -> anyhow::Result<()> {
        let replica_sets: Vec<ReplicaSet> = self
            .storage
            .list(&build_prefix("replicasets", None))
            .await?;
        for replica_set in replica_sets {
            let Some(owner) = controller_owner(&replica_set.metadata) else {
                continue;
            };
            if owner.kind == "Deployment" {
                let key = build_key(
                    "deployments",
                    Some(&replica_set.metadata.namespace),
                    &owner.name,
                );
                if matches!(
                    self.storage.get::<Deployment>(&key).await,
                    Err(StoreError::NotFound(_))
                ) {
                    delete_if_exists(
                        &self.storage,
                        "replicasets",
                        Some(&replica_set.metadata.namespace),
                        &replica_set.metadata.name,
                    )
                    .await?;
                }
            }
        }

        let pods: Vec<Pod> = self.storage.list(&build_prefix("pods", None)).await?;
        for pod in pods {
            let Some(owner) = controller_owner(&pod.metadata) else {
                continue;
            };
            if owner.kind == "ReplicaSet" {
                let key = build_key("replicasets", Some(&pod.metadata.namespace), &owner.name);
                if matches!(
                    self.storage.get::<ReplicaSet>(&key).await,
                    Err(StoreError::NotFound(_))
                ) {
                    delete_if_exists(
                        &self.storage,
                        "pods",
                        Some(&pod.metadata.namespace),
                        &pod.metadata.name,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }
}

fn replica_set_for_deployment(deployment: &Deployment, name: &str) -> ReplicaSet {
    let mut labels = deployment
        .spec
        .template
        .metadata
        .labels
        .clone()
        .unwrap_or_default();
    labels.insert(
        "boss.io/deployment".to_string(),
        deployment.metadata.name.clone(),
    );
    labels.insert(
        "boss.io/template-hash".to_string(),
        template_hash(deployment),
    );

    let mut template = deployment.spec.template.clone();
    template.metadata.labels = Some(labels);

    Object {
        type_meta: TypeMeta {
            api_version: ReplicaSetSpec::API_VERSION.to_string(),
            kind: ReplicaSetSpec::KIND.to_string(),
        },
        metadata: ObjectMeta {
            name: name.to_string(),
            namespace: deployment.metadata.namespace.clone(),
            labels: Some(BTreeMap::from([
                (
                    "boss.io/deployment".to_string(),
                    deployment.metadata.name.clone(),
                ),
                (
                    "boss.io/template-hash".to_string(),
                    template_hash(deployment),
                ),
            ])),
            owner_references: vec![owner_ref(deployment)],
            ..Default::default()
        },
        spec: ReplicaSetSpec {
            replicas: deployment.spec.replicas,
            selector: deployment.spec.selector.clone(),
            template,
        },
        status: None,
    }
}

fn pod_for_replica_set(replica_set: &ReplicaSet, index: usize) -> Pod {
    let mut labels = replica_set
        .spec
        .template
        .metadata
        .labels
        .clone()
        .unwrap_or_default();
    labels.insert(
        "boss.io/replicaset".to_string(),
        replica_set.metadata.name.clone(),
    );

    Object {
        type_meta: TypeMeta {
            api_version: PodSpec::API_VERSION.to_string(),
            kind: PodSpec::KIND.to_string(),
        },
        metadata: ObjectMeta {
            name: format!("{}-{index}", replica_set.metadata.name),
            namespace: replica_set.metadata.namespace.clone(),
            labels: Some(labels),
            owner_references: vec![owner_ref(replica_set)],
            ..Default::default()
        },
        spec: replica_set.spec.template.spec.clone(),
        status: None,
    }
}

fn current_replica_set_name(deployment: &Deployment) -> String {
    format!("{}-{}", deployment.metadata.name, template_hash(deployment))
}

fn template_hash(deployment: &Deployment) -> String {
    let value = serde_json::to_vec(&deployment.spec.template).unwrap_or_default();
    let hash = value.iter().fold(0xcbf29ce484222325u64, |acc, byte| {
        let xored = acc ^ (*byte as u64);
        xored.wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

fn owner_ref<T: Resource>(obj: &Object<T>) -> OwnerReference {
    OwnerReference {
        api_version: T::API_VERSION.to_string(),
        kind: T::KIND.to_string(),
        name: obj.metadata.name.clone(),
        uid: obj.metadata.uid.clone().unwrap_or_default(),
        controller: Some(true),
    }
}

fn controller_owner(meta: &ObjectMeta) -> Option<&OwnerReference> {
    meta.owner_references
        .iter()
        .find(|owner| owner.controller.unwrap_or(false))
}

fn controlled_by<T: Resource>(obj: &Object<T>, owner: &OwnerReference) -> bool {
    obj.metadata.owner_references.iter().any(|candidate| {
        candidate.controller.unwrap_or(false)
            && candidate.kind == owner.kind
            && candidate.name == owner.name
            && candidate.uid == owner.uid
    })
}

async fn owned_replica_sets(
    storage: &StorageBackend,
    deployment: &Deployment,
) -> anyhow::Result<Vec<ReplicaSet>> {
    let owner = owner_ref(deployment);
    let replica_sets: Vec<ReplicaSet> = storage
        .list(&build_prefix(
            "replicasets",
            Some(&deployment.metadata.namespace),
        ))
        .await?;
    Ok(replica_sets
        .into_iter()
        .filter(|replica_set| controlled_by(replica_set, &owner))
        .collect())
}

async fn owned_pods(
    storage: &StorageBackend,
    replica_set: &ReplicaSet,
) -> anyhow::Result<Vec<Pod>> {
    let owner = owner_ref(replica_set);
    let pods: Vec<Pod> = storage
        .list(&build_prefix("pods", Some(&replica_set.metadata.namespace)))
        .await?;
    Ok(pods
        .into_iter()
        .filter(|pod| controlled_by(pod, &owner))
        .collect())
}

fn validate_selector_matches_template(
    selector: &boss_api::LabelSelector,
    labels: Option<&BTreeMap<String, String>>,
) -> anyhow::Result<()> {
    let labels = labels.cloned().unwrap_or_default();
    if selector.matches(&labels) {
        Ok(())
    } else {
        Err(anyhow::anyhow!("selector does not match template labels"))
    }
}

fn deployment_conditions(
    deployment: &Deployment,
    status: &DeploymentStatus,
    degraded: bool,
    reason: &str,
    message: &str,
) -> Vec<Condition> {
    vec![
        Condition::new(
            "Reconciling",
            "False",
            reason,
            message,
            deployment.metadata.generation,
        ),
        Condition::new(
            "Progressing",
            if status.replicas == deployment.spec.replicas {
                "True"
            } else {
                "Unknown"
            },
            reason,
            message,
            deployment.metadata.generation,
        ),
        Condition::new(
            "Available",
            if status.available_replicas > 0 {
                "True"
            } else {
                "False"
            },
            if status.available_replicas > 0 {
                "MinimumAvailable"
            } else {
                "MinimumUnavailable"
            },
            if status.available_replicas > 0 {
                "at least one replica is available"
            } else {
                "no replicas are available"
            },
            deployment.metadata.generation,
        ),
        Condition::new(
            "Degraded",
            if degraded { "True" } else { "False" },
            if degraded {
                reason
            } else {
                "NoDegradedCondition"
            },
            if degraded {
                message
            } else {
                "no blocking controller error"
            },
            deployment.metadata.generation,
        ),
    ]
}

fn replicaset_conditions(
    replica_set: &ReplicaSet,
    status: &ReplicaSetStatus,
    degraded: bool,
    reason: &str,
    message: &str,
) -> Vec<Condition> {
    vec![
        Condition::new(
            "Reconciling",
            "False",
            reason,
            message,
            replica_set.metadata.generation,
        ),
        Condition::new(
            "Available",
            if status.available_replicas > 0 || replica_set.spec.replicas == 0 {
                "True"
            } else {
                "False"
            },
            if status.available_replicas > 0 || replica_set.spec.replicas == 0 {
                "MinimumAvailable"
            } else {
                "MinimumUnavailable"
            },
            if status.available_replicas > 0 || replica_set.spec.replicas == 0 {
                "desired availability is satisfied"
            } else {
                "no replicas are available"
            },
            replica_set.metadata.generation,
        ),
        Condition::new(
            "Degraded",
            if degraded { "True" } else { "False" },
            if degraded {
                reason
            } else {
                "NoDegradedCondition"
            },
            if degraded {
                message
            } else {
                "no blocking controller error"
            },
            replica_set.metadata.generation,
        ),
    ]
}

fn deployment_status_semantically_equal(left: &DeploymentStatus, right: &DeploymentStatus) -> bool {
    left.observed_generation == right.observed_generation
        && left.replicas == right.replicas
        && left.ready_replicas == right.ready_replicas
        && left.updated_replicas == right.updated_replicas
        && left.available_replicas == right.available_replicas
        && conditions_semantically_equal(&left.conditions, &right.conditions)
}

fn replicaset_status_semantically_equal(left: &ReplicaSetStatus, right: &ReplicaSetStatus) -> bool {
    left.observed_generation == right.observed_generation
        && left.replicas == right.replicas
        && left.ready_replicas == right.ready_replicas
        && left.available_replicas == right.available_replicas
        && conditions_semantically_equal(&left.conditions, &right.conditions)
}

fn conditions_semantically_equal(left: &[Condition], right: &[Condition]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.kind == right.kind
                && left.status == right.status
                && left.reason == right.reason
                && left.message == right.message
                && left.observed_generation == right.observed_generation
        })
}

async fn update_status<T: Resource>(
    storage: &StorageBackend,
    key: &str,
    mut object: Object<T>,
) -> anyhow::Result<()> {
    for _ in 0..5 {
        match storage.update::<Object<T>>(key, &object).await {
            Ok(_) => return Ok(()),
            Err(StoreError::Conflict(_)) => {
                let mut latest: Object<T> = storage.get(key).await?;
                latest.status = object.status.clone();
                object = latest;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(anyhow::anyhow!("status update retries exhausted"))
}

async fn delete_if_exists(
    storage: &StorageBackend,
    resource: &str,
    namespace: Option<&str>,
    name: &str,
) -> anyhow::Result<()> {
    let key = build_key(resource, namespace, name);
    match storage.delete(&key).await {
        Ok(_) => Ok(()),
        Err(StoreError::NotFound(_)) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boss_api::{
        Container, DeploymentSpec, LabelSelector, PodTemplateSpec, RestartPolicy, SandboxArtifact,
        SandboxNetwork, SandboxRequirements,
    };
    use boss_store::MemoryStorage;

    fn storage() -> Arc<StorageBackend> {
        Arc::new(StorageBackend::Memory(MemoryStorage::arc()))
    }

    fn deployment(name: &str, replicas: i32) -> Deployment {
        let labels = BTreeMap::from([("app".to_string(), name.to_string())]);
        Object {
            type_meta: TypeMeta {
                api_version: DeploymentSpec::API_VERSION.to_string(),
                kind: DeploymentSpec::KIND.to_string(),
            },
            metadata: ObjectMeta {
                name: name.to_string(),
                namespace: "default".to_string(),
                uid: Some(format!("{name}-uid")),
                generation: 1,
                ..Default::default()
            },
            spec: DeploymentSpec {
                replicas,
                selector: LabelSelector {
                    match_labels: Some(labels.clone()),
                    ..Default::default()
                },
                template: PodTemplateSpec {
                    metadata: ObjectMeta {
                        labels: Some(labels),
                        ..Default::default()
                    },
                    spec: PodSpec {
                        sandbox_class: Some("process".to_string()),
                        sandbox: Some(SandboxRequirements {
                            artifact: Some(SandboxArtifact {
                                kind: "executable".to_string(),
                                path: Some("sleep".to_string()),
                                ..Default::default()
                            }),
                            network: Some(SandboxNetwork {
                                mode: Some("host".to_string()),
                            }),
                            isolation: Some("sharedHost".to_string()),
                            ..Default::default()
                        }),
                        containers: vec![Container {
                            name: "sleep".to_string(),
                            command: vec!["sleep".to_string()],
                            args: vec!["300".to_string()],
                            ..Default::default()
                        }],
                        restart_policy: RestartPolicy::Never,
                        ..Default::default()
                    },
                },
                ..Default::default()
            },
            status: None,
        }
    }

    #[tokio::test]
    async fn deployment_controller_creates_owned_replicaset() {
        let storage = storage();
        let deployment = deployment("demo", 2);
        let deployment_key = build_key("deployments", Some("default"), "demo");
        storage.create(&deployment_key, &deployment).await.unwrap();

        DeploymentController::new(storage.clone())
            .reconcile(&deployment_key)
            .await
            .unwrap();

        let replica_sets: Vec<ReplicaSet> = storage
            .list(&build_prefix("replicasets", Some("default")))
            .await
            .unwrap();
        assert_eq!(replica_sets.len(), 1);
        assert_eq!(replica_sets[0].spec.replicas, 2);
        assert_eq!(
            replica_sets[0].spec.template.spec.sandbox_class.as_deref(),
            Some("process")
        );
        assert!(controlled_by(&replica_sets[0], &owner_ref(&deployment)));
    }

    #[tokio::test]
    async fn replicaset_controller_creates_owned_pods_with_unbound_intent() {
        let storage = storage();
        let deployment = deployment("demo", 2);
        let replica_set =
            replica_set_for_deployment(&deployment, &current_replica_set_name(&deployment));
        let replica_set_key = build_key("replicasets", Some("default"), &replica_set.metadata.name);
        storage
            .create(&replica_set_key, &replica_set)
            .await
            .unwrap();

        ReplicaSetController::new(storage.clone())
            .reconcile(&replica_set_key)
            .await
            .unwrap();

        let pods: Vec<Pod> = storage
            .list(&build_prefix("pods", Some("default")))
            .await
            .unwrap();
        assert_eq!(pods.len(), 2);
        for pod in pods {
            assert_eq!(pod.spec.node_name, None);
            assert_eq!(pod.spec.sandbox_class.as_deref(), Some("process"));
            assert!(controlled_by(&pod, &owner_ref(&replica_set)));
        }
    }

    #[tokio::test]
    async fn semantic_status_comparison_ignores_transition_time() {
        let left = DeploymentStatus {
            observed_generation: 1,
            replicas: 1,
            conditions: vec![Condition {
                kind: "Available".to_string(),
                status: "True".to_string(),
                reason: Some("MinimumAvailable".to_string()),
                message: Some("ready".to_string()),
                observed_generation: Some(1),
                last_transition_time: Some("one".to_string()),
            }],
            ..Default::default()
        };
        let mut right = left.clone();
        right.conditions[0].last_transition_time = Some("two".to_string());

        assert!(deployment_status_semantically_equal(&left, &right));
    }
}
