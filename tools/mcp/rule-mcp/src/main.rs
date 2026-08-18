use std::path::PathBuf;

use rule_api::RuleStore;
use rule_mcp::server;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("rule_mcp=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    let index_root = resolve_index_root();

    RuleStore::open_or_init(&index_root).unwrap_or_else(|err| {
        eprintln!(
            "Failed to open rule store at {}: {err}",
            index_root.display()
        );
        std::process::exit(1);
    });

    eprintln!("rule-mcp starting (store: {})", index_root.display());

    if let Err(err) = server::run_mcp_server(index_root).await {
        eprintln!("Fatal error: {err}");
        std::process::exit(1);
    }
}

fn resolve_index_root() -> PathBuf {
    if let Ok(path) = std::env::var("RULE_INDEX_ROOT") {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("TICKET_INDEX_ROOT") {
        return PathBuf::from(path);
    }
    let cwd_rule = std::env::current_dir().ok().map(|dir| dir.join(".rule"));
    if let Some(path) = cwd_rule.filter(|path| path.exists()) {
        return path;
    }
    if let Ok(home) =
        std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))
    {
        return PathBuf::from(home).join(".rule-index");
    }
    PathBuf::from(".rule")
}
