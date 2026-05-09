use std::sync::Arc;

use ticket_api::{
    model::filesystem::ScanRoot,
    storage::store::TicketStore,
};

use crate::serve::{
    AppState,
    StreamBroker,
    WorkspaceRegistry,
};

#[path = "tests/lifecycle.rs"]
mod lifecycle;
#[path = "tests/listing.rs"]
mod listing;
#[path = "tests/mutations.rs"]
mod mutations;

fn make_store(dir: &std::path::Path) -> Arc<TicketStore> {
    let store = Arc::new(TicketStore::open(dir).expect("open store"));
    store
        .add_scan_root(ScanRoot {
            path: dir.join("tickets"),
            label: "default".into(),
        })
        .expect("add scan root");
    store
}

fn make_state(store: Arc<TicketStore>) -> AppState {
    AppState::new(
        Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&store))),
        Arc::new(StreamBroker::new()),
    )
}
