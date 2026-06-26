//! Node agent. Watches pods bound to this node, drives the runtime provider,
//! reports pod status, and heartbeats the node through a small sync loop,
//! per-pod workers, liveness polling, and status reporting.

pub mod bosslet;
pub mod client;

pub use bosslet::Bosslet;
pub use client::ApiServerClient;
