mod server;

use std::path::PathBuf;

use ticket_api::storage::store::TicketStore;
use ticket_api::workspace::WorkspaceConfig;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("ticket_mcp=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    let index_root = std::env::var("TICKET_INDEX_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let (path, _source) = ticket_api::workspace::resolve_workspace();
            path
        });

    TicketStore::open(&index_root).unwrap_or_else(|e| {
        eprintln!("Failed to open ticket store at {}: {e}", index_root.display());
        std::process::exit(1);
    });

    let config = WorkspaceConfig::load();
    let workspace_names: Vec<String> = if config.workspaces.is_empty() {
        vec!["default".to_string()]
    } else {
        config.workspaces.keys().cloned().collect()
    };

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
