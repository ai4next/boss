use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use boss_api::{Pod, PodSpec};
use boss_store::{Storage, build_prefix};

use crate::error::ApiResult;
use crate::generic;
use crate::state::AppState;
use crate::watch;

pub const RESOURCE: &str = "pods";

/// POST /api/v1/namespaces/{namespace}/pods
pub async fn create(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(body): Json<Pod>,
) -> ApiResult<impl IntoResponse> {
    let pod = generic::create::<PodSpec>(&state, RESOURCE, Some(&namespace), body).await?;
    Ok((StatusCode::CREATED, Json(pod)))
}

/// GET /api/v1/namespaces/{namespace}/pods[?watch=true&resourceVersion=N]
pub async fn list(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(params): Query<BTreeMap<String, String>>,
) -> ApiResult<Response> {
    let (watch, rv) = watch::parse_watch_params(&params);
    if watch {
        let prefix = build_prefix(RESOURCE, Some(&namespace));
        let stream = state.storage.watch(&prefix, rv).await?;
        Ok(watch::watch_response(stream))
    } else {
        let list = generic::list::<PodSpec>(&state, RESOURCE, Some(&namespace)).await?;
        Ok(Json(list).into_response())
    }
}

/// GET /api/v1/namespaces/{namespace}/pods/{name}
pub async fn get(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    let pod = generic::get::<PodSpec>(&state, RESOURCE, Some(&namespace), &name).await?;
    Ok(Json(pod))
}

/// PUT /api/v1/namespaces/{namespace}/pods/{name}
pub async fn update(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(body): Json<Pod>,
) -> ApiResult<impl IntoResponse> {
    let pod = generic::update::<PodSpec>(&state, RESOURCE, Some(&namespace), &name, body).await?;
    Ok(Json(pod))
}

/// PUT /api/v1/namespaces/{namespace}/pods/{name}/status
pub async fn update_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(body): Json<Pod>,
) -> ApiResult<impl IntoResponse> {
    let pod = generic::update::<PodSpec>(&state, RESOURCE, Some(&namespace), &name, body).await?;
    Ok(Json(pod))
}

/// DELETE /api/v1/namespaces/{namespace}/pods/{name}
pub async fn delete(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    generic::delete::<PodSpec>(&state, RESOURCE, Some(&namespace), &name).await?;
    Ok(StatusCode::OK)
}

/// GET /api/v1/pods[?watch=true] — list/watch across all namespaces.
pub async fn list_all(
    State(state): State<AppState>,
    Query(params): Query<BTreeMap<String, String>>,
) -> ApiResult<Response> {
    let (watch, rv) = watch::parse_watch_params(&params);
    if watch {
        let prefix = build_prefix(RESOURCE, None);
        let stream = state.storage.watch(&prefix, rv).await?;
        Ok(watch::watch_response(stream))
    } else {
        let list = generic::list::<PodSpec>(&state, RESOURCE, None).await?;
        Ok(Json(list).into_response())
    }
}
