use thiserror::Error;

/// Top-level error type used across the boss crates.
#[derive(Debug, Error)]
pub enum BossError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("conflict: optimistic concurrency mismatch for {0}")]
    Conflict(String),
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("internal: {0}")]
    Internal(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl BossError {
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, BossError>;
