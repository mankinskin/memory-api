use std::path::PathBuf;

use spec_api::SpecStore;
use spec_http::{ServeConfig, SpecAppState, start_server};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("spec_http=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut port: u16 = 4001;
    let mut host = "127.0.0.1".to_string();
    let mut index_root: Option<String> = None;

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                port = args[i].parse().expect("invalid port");
            }
            "--host" => {
                i += 1;
                host = args[i].clone();
            }
            "--index-root" => {
                i += 1;
                index_root = Some(args[i].clone());
            }
            _ => {}
        }
        i += 1;
    }

    let root = index_root
        .map(PathBuf::from)
        .or_else(|| std::env::var("SPEC_INDEX_ROOT").ok().map(PathBuf::from))
        .or_else(|| std::env::var("TICKET_INDEX_ROOT").ok().map(PathBuf::from))
        .unwrap_or_else(|| {
            let cwd = std::env::current_dir().expect("cwd");
            let spec_dir = cwd.join(".spec");
            if spec_dir.exists() {
                spec_dir
            } else {
                cwd.join(".ticket")
            }
        });

    let store = SpecStore::open(&root).unwrap_or_else(|e| {
        eprintln!("Failed to open spec store at {}: {e}", root.display());
        std::process::exit(1);
    });

    let state = SpecAppState::new(store);
    let config = ServeConfig { host, port };

    if let Err(err) = start_server(config, state).await {
        eprintln!("Fatal error: {err}");
        std::process::exit(1);
    }
}
