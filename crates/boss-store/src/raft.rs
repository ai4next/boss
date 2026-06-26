//! Raft-backed storage (Phase 5). Currently a stub.
//!
//! TODO: openraft FSM storing `BTreeMap<key, (value, rv)>`, multi-node
//! initialize/join via `add_learner`, snapshot/restore, linearizable
//! read-index. The `WatchBus` is fed from the FSM apply path.

use boss_api::ResourceVersion;

use crate::storage::{WatchEvent, WatchStream};

/// Placeholder raft registry. All methods panic — use `MemoryStorage` until
/// Phase 5 lands.
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

#[allow(unused_variables)]
impl RaftStorage {
    pub async fn current_revision(&self) -> ResourceVersion {
        unimplemented!("raft storage is Phase 5; use MemoryStorage")
    }

    pub async fn watch(&self, prefix: &str, start_rv: ResourceVersion) -> WatchStream {
        unimplemented!("raft storage is Phase 5; use MemoryStorage")
    }
}

// Prevent unused warnings while the feature compiles.
const _: Option<WatchEvent> = None;
