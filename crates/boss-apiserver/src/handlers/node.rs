use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use boss_api::{Node, NodeSpec};
use boss_store::{Storage, build_prefix};

use crate::error::ApiResult;
use crate::generic;
use crate::state::AppState;
use crate::watch;

pub const RESOURCE: &str = "nodes";

/// POST /api/v1/nodes
pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<Node>,
) -> ApiResult<impl IntoResponse> {
    let node = generic::create::<NodeSpec>(&state, RESOURCE, None, body).await?;
    Ok((StatusCode::CREATED, Json(node)))
}

/// GET /api/v1/nodes[?watch=true]
pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<BTreeMap<String, String>>,
) -> ApiResult<Response> {
    let (watch, rv) = watch::parse_watch_params(&params);
    if watch {
        let prefix = build_prefix(RESOURCE, None);
        let stream = state.storage.watch(&prefix, rv).await?;
        Ok(watch::watch_response(stream))
    } else {
        let list = generic::list::<NodeSpec>(&state, RESOURCE, None).await?;
        Ok(Json(list).into_response())
    }
}

/// GET /api/v1/nodes/{name}
pub async fn get(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let node = generic::get::<NodeSpec>(&state, RESOURCE, None, &name).await?;
    Ok(Json(node))
}

/// PUT /api/v1/nodes/{name}
pub async fn update(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<Node>,
) -> ApiResult<impl IntoResponse> {
    let node = generic::update::<NodeSpec>(&state, RESOURCE, None, &name, body).await?;
    Ok(Json(node))
}

/// PUT /api/v1/nodes/{name}/status
pub async fn update_status(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<Node>,
) -> ApiResult<impl IntoResponse> {
    let node = generic::update_status::<NodeSpec>(&state, RESOURCE, None, &name, body).await?;
    Ok(Json(node))
}

/// DELETE /api/v1/nodes/{name}
pub async fn delete(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<impl IntoResponse> {
    generic::delete::<NodeSpec>(&state, RESOURCE, None, &name).await?;
    Ok(StatusCode::OK)
}
