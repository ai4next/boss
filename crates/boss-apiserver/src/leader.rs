//! Single-node leadership gate.

/// Coordinates leadership for the single-node build. Multi-replica leadership
/// is deliberately outside this final local build and should be added with a
/// persistent store plus write fencing.
pub struct LeaseCoordinator {
    leader: bool,
}

impl LeaseCoordinator {
    pub fn new() -> Self {
        Self { leader: true }
    }

    pub fn is_leader(&self) -> bool {
        self.leader
    }
}

impl Default for LeaseCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
