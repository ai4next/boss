use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use boss_api::{Deployment, DeploymentSpec};
use boss_store::{Storage, build_prefix};

use crate::error::ApiResult;
use crate::generic;
use crate::state::AppState;
use crate::watch;

pub const RESOURCE: &str = "deployments";

pub async fn create(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(body): Json<Deployment>,
) -> ApiResult<impl IntoResponse> {
    let deployment =
        generic::create::<DeploymentSpec>(&state, RESOURCE, Some(&namespace), body).await?;
    Ok((StatusCode::CREATED, Json(deployment)))
}

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
        let list = generic::list::<DeploymentSpec>(&state, RESOURCE, Some(&namespace)).await?;
        Ok(Json(list).into_response())
    }
}

pub async fn get(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    let deployment =
        generic::get::<DeploymentSpec>(&state, RESOURCE, Some(&namespace), &name).await?;
    Ok(Json(deployment))
}

pub async fn update(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(body): Json<Deployment>,
) -> ApiResult<impl IntoResponse> {
    let deployment =
        generic::update::<DeploymentSpec>(&state, RESOURCE, Some(&namespace), &name, body).await?;
    Ok(Json(deployment))
}

pub async fn update_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(body): Json<Deployment>,
) -> ApiResult<impl IntoResponse> {
    let deployment =
        generic::update_status::<DeploymentSpec>(&state, RESOURCE, Some(&namespace), &name, body)
            .await?;
    Ok(Json(deployment))
}

pub async fn delete(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    generic::delete::<DeploymentSpec>(&state, RESOURCE, Some(&namespace), &name).await?;
    Ok(StatusCode::OK)
}
