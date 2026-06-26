//! Storage layer: a generic `Storage` trait over a consistent KV store with
//! optimistic concurrency, an in-memory implementation, and a watch bus.
//! A raft-backed implementation is stubbed behind the `raft` feature (Phase 5).
//!
//! Design: flat string keys (`/registry/{type}/{ns}/{name}`), generic
//! `T: Serialize + DeserializeOwned` values, CAS via
//! `metadata.resourceVersion`, and `BoxStream`-based watch.

pub mod error;
pub mod key;
pub mod memory;
pub mod storage;
pub mod watch;

#[cfg(feature = "raft")]
pub mod raft;

pub use key::{build_key, build_prefix};
pub use memory::MemoryStorage;
pub use storage::{Storage, StorageBackend, WatchEvent, WatchStream};
pub use watch::WatchBus;
