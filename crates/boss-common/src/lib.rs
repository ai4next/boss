//! Shared utilities: errors, logging init, id generation, time helpers.

pub mod error;
pub mod id;
pub mod log;
pub mod time;

pub use error::{BossError, Result};
