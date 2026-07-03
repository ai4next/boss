//! Raft-backed storage placeholder.
//!
//! The single-node final build uses `MemoryStorage`. This type exists so the
//! feature compiles and callers receive explicit errors instead of panics.

use boss_api::ResourceVersion;

use crate::error::StoreResult;
use crate::storage::{WatchEvent, WatchStream};

/// Placeholder raft registry for future multi-node storage.
pub struct RaftStorage;

impl RaftStorage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RaftStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl RaftStorage {
    pub async fn current_revision(&self) -> StoreResult<ResourceVersion> {
        Err(boss_common::BossError::NotImplemented(
            "raft storage is not included in the single-node build; use MemoryStorage".into(),
        ))
    }

    pub async fn watch(
        &self,
        _prefix: &str,
        _start_rv: ResourceVersion,
    ) -> StoreResult<WatchStream> {
        Err(boss_common::BossError::NotImplemented(
            "raft storage is not included in the single-node build; use MemoryStorage".into(),
        ))
    }
}

// Prevent unused warnings while the feature compiles.
const _: Option<WatchEvent> = None;
