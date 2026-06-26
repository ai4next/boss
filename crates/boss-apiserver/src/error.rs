use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;

/// API error, mapped to Boss `Status` JSON responses.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Store(#[from] boss_common::BossError),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid: {0}")]
    Invalid(String),

    #[error("internal: {0}")]
    Internal(String),
}

#[derive(Serialize)]
struct Status {
    #[serde(rename = "apiVersion")]
    api_version: &'static str,
    kind: &'static str,
    status: &'static str,
    message: String,
    reason: String,
    code: u16,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (code, reason, message) = match &self {
            ApiError::Store(e) => match e {
                boss_common::BossError::NotFound(m) => {
                    (StatusCode::NOT_FOUND, "NotFound".to_string(), m.clone())
                }
                boss_common::BossError::AlreadyExists(m) => {
                    (StatusCode::CONFLICT, "AlreadyExists".to_string(), m.clone())
                }
                boss_common::BossError::Conflict(m) => {
                    (StatusCode::CONFLICT, "Conflict".to_string(), m.clone())
                }
                boss_common::BossError::Invalid(m) => (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Invalid".to_string(),
                    m.clone(),
                ),
                boss_common::BossError::NotImplemented(m) => (
                    StatusCode::NOT_IMPLEMENTED,
                    "NotImplemented".to_string(),
                    m.clone(),
                ),
                other => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError".to_string(),
                    other.to_string(),
                ),
            },
            ApiError::BadRequest(m) => {
                (StatusCode::BAD_REQUEST, "BadRequest".to_string(), m.clone())
            }
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, "NotFound".to_string(), m.clone()),
            ApiError::Invalid(m) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Invalid".to_string(),
                m.clone(),
            ),
            ApiError::Internal(m) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError".to_string(),
                m.clone(),
            ),
        };
        let status = Status {
            api_version: "boss.io/v1",
            kind: "Status",
            status: "Failure",
            message,
            reason,
            code: code.as_u16(),
        };
        (code, Json(status)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
