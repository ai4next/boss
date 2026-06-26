//! Core data model for boss. Pure serde types — no async, no I/O.

pub mod deployment;
pub mod lease;
pub mod meta;
pub mod node;
pub mod object;
pub mod pod;
pub mod replicaset;
pub mod resource_version;
pub mod selector;
pub mod watch;

pub use deployment::*;
pub use lease::*;
pub use meta::*;
pub use node::*;
pub use object::*;
pub use pod::*;
pub use replicaset::*;
pub use resource_version::*;
pub use selector::*;
pub use watch::*;
