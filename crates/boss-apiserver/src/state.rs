use std::sync::Arc;

use boss_store::StorageBackend;

/// Shared apiserver state.
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<StorageBackend>,
    pub leader: Arc<crate::leader::LeaseCoordinator>,
}

impl AppState {
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self {
            storage,
            leader: Arc::new(crate::leader::LeaseCoordinator::new()),
        }
    }
}
