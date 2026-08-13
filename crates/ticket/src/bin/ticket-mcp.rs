#![recursion_limit = "256"]

use ticket::server::{
    self,
    open_canonical_store,
};

use std::path::PathBuf;

use memory_kernel::runtime::init_transport_tracing;

#[tokio::main]
async fn main() {
    init_transport_tracing("ticket_mcp=info", None, None, "warn");

    let index_root = std::env::var("TICKET_INDEX_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let (path, _source) = ticket_api::workspace::resolve_workspace();
            path
        });

    let store = open_canonical_store(&index_root).unwrap_or_else(|e| {
        eprintln!(
            "Failed to open ticket store at {}: {e}",
            index_root.display()
        );
        std::process::exit(1);
    });
    let index_root = store.index_root.clone();
    drop(store);

    let workspace_names = vec!["default".to_string()];

    eprintln!(
        "ticket-mcp starting (store: {}, workspaces: {:?})",
        index_root.display(),
        workspace_names,
    );

    if let Err(err) = server::run_mcp_server(index_root).await {
        eprintln!("Fatal error: {err}");
        std::process::exit(1);
    }
}
