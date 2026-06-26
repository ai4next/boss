//! Apiserver: axum HTTP layer over the storage backend. Implements Boss
//! CRUD + watch (newline-delimited JSON) for Pods and Nodes, with optimistic
//! concurrency (CAS) via `metadata.resourceVersion`.

pub mod app;
pub mod error;
pub mod generic;
pub mod handlers;
pub mod leader;
pub mod state;
pub mod watch;

pub use app::{router, serve};
pub use state::AppState;
