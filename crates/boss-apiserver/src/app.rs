use axum::Router;
use axum::routing::{get, put};
use tower_http::trace::TraceLayer;

use crate::handlers::{deployment, node, pod, replicaset};
use crate::state::AppState;

/// Build the apiserver router.
pub fn router(state: AppState) -> Router {
    let core = Router::new()
        // Pods (namespaced)
        .route(
            "/api/v1/namespaces/{namespace}/pods",
            get(pod::list).post(pod::create),
        )
        .route(
            "/api/v1/namespaces/{namespace}/pods/{name}",
            get(pod::get).put(pod::update).delete(pod::delete),
        )
        .route(
            "/api/v1/namespaces/{namespace}/pods/{name}/status",
            put(pod::update_status),
        )
        // Pods (all namespaces)
        .route("/api/v1/pods", get(pod::list_all))
        // Deployments (namespaced)
        .route(
            "/api/v1/namespaces/{namespace}/deployments",
            get(deployment::list).post(deployment::create),
        )
        .route(
            "/api/v1/namespaces/{namespace}/deployments/{name}",
            get(deployment::get)
                .put(deployment::update)
                .delete(deployment::delete),
        )
        .route(
            "/api/v1/namespaces/{namespace}/deployments/{name}/status",
            put(deployment::update_status),
        )
        // ReplicaSets (namespaced)
        .route(
            "/api/v1/namespaces/{namespace}/replicasets",
            get(replicaset::list).post(replicaset::create),
        )
        .route(
            "/api/v1/namespaces/{namespace}/replicasets/{name}",
            get(replicaset::get)
                .put(replicaset::update)
                .delete(replicaset::delete),
        )
        .route(
            "/api/v1/namespaces/{namespace}/replicasets/{name}/status",
            put(replicaset::update_status),
        )
        // Nodes (cluster-scoped)
        .route("/api/v1/nodes", get(node::list).post(node::create))
        .route(
            "/api/v1/nodes/{name}",
            get(node::get).put(node::update).delete(node::delete),
        )
        .route("/api/v1/nodes/{name}/status", put(node::update_status));

    Router::new()
        .merge(core)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Bind and serve the apiserver on `addr`.
pub async fn serve(addr: &str, state: AppState) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("apiserver listening on {addr}");
    axum::serve(listener, router(state)).await?;
    Ok(())
}
