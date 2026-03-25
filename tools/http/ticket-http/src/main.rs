//! Standalone binary for the ticket HTTP server.
//!
//! Usage:
//!   ticket-http --port 4000 [--host 127.0.0.1] [--workspace default]

use ticket_api::workspace::WorkspaceConfig;
use ticket_api::storage::store::TicketStore;
use ticket_http::serve::{ServeConfig, WorkspaceRegistry};

fn main() {
    let mut port: u16 = 4000;
    let mut host = "127.0.0.1".to_string();
    let mut workspace: Option<String> = None;
    let mut index_root: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                if let Some(v) = args.next() {
                    port = v.parse().unwrap_or(port);
                }
            }
            "--host" => {
                if let Some(v) = args.next() {
                    host = v;
                }
            }
            "--workspace" => {
                workspace = args.next();
            }
            "--index-root" => {
                index_root = args.next();
            }
            _ => {}
        }
    }

    let store = {
        let root = index_root
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                let (path, _source) = ticket_api::workspace::resolve_workspace();
                path
            });
        TicketStore::open(&root).expect("failed to open ticket store")
    };

    let registry = if workspace.is_some() {
        WorkspaceRegistry::single_opened(std::sync::Arc::new(store))
    } else {
        let config = WorkspaceConfig::load();
        if config.workspaces.is_empty() {
            WorkspaceRegistry::single_opened(std::sync::Arc::new(store))
        } else {
            WorkspaceRegistry::from_config(&config)
        }
    };

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
