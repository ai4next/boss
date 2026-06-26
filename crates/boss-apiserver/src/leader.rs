//! Leader election via lease (Phase 5). Skeleton: single-node is always leader.

/// Coordinates leadership. In the skeleton (single replica) every instance
/// considers itself the leader, so writes are always served.
///
/// TODO: write a `Lease{holder, renewTime, leaseDuration}` resource to storage,
/// renew before expiry, and fence writes (only the leader may mutate). Other
/// replicas return 503 and proxy/redirect.
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
