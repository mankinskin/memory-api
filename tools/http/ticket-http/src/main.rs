//! Standalone binary for the ticket HTTP server.
//!
//! Usage:
//!   ticket-http --port 4000 [--host 127.0.0.1] [--index-root <path>]

use ticket_api::storage::store::TicketStore;
use ticket_http::serve::{
    ServeConfig,
    WorkspaceRegistry,
};

fn main() {
    let mut port: u16 = 4000;
    let mut host = "127.0.0.1".to_string();
    let mut index_root: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" =>
                if let Some(v) = args.next() {
                    port = v.parse().unwrap_or(port);
                },
            "--host" =>
                if let Some(v) = args.next() {
                    host = v;
                },
            "--index-root" => {
                index_root = args.next();
            },
            _ => {},
        }
    }

    let root = index_root.map(std::path::PathBuf::from).unwrap_or_else(|| {
        let (path, _source) = ticket_api::workspace::resolve_workspace();
        path
    });
    let workspace_root =
        ticket_api::workspace::resolve_workspace_root_from_store_root(
            &root,
            ticket_api::workspace::TICKET_INDEX_DIR,
        );
    let store = TicketStore::open(&root).expect("failed to open ticket store");
    if ticket_http::serve::register_descendant_scan_roots(
        &store,
        &workspace_root,
    )
    .expect("failed to register descendant workspaces")
    {
        store
            .scan(true)
            .expect("failed to reindex ticket store after registering descendant workspaces");
    }

    let registry = WorkspaceRegistry::single_opened(std::sync::Arc::new(store));

    let config = ServeConfig { host, port };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start tokio runtime");

    rt.block_on(async {
        ticket_http::start_server(config, registry)
            .await
            .expect("server error");
    });
}
