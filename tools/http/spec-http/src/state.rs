use std::sync::Arc;

use spec_api::SpecStore;
use tokio::sync::Mutex;

/// Shared application state for spec-http handlers.
///
/// SpecStore needs `&mut self` for create/update/delete/scan,
/// so we wrap it in an async Mutex. The Mutex is held only for
/// the duration of each handler call.
#[derive(Clone)]
pub struct SpecAppState {
    pub store: Arc<Mutex<SpecStore>>,
}

impl SpecAppState {
    pub fn new(store: SpecStore) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
        }
    }
}
