use std::path::PathBuf;

use memory_api::runtime::init_transport_tracing;
use doc_http::{
    DocAppState,
    ServeConfig,
    start_server,
};

#[tokio::main]
async fn main() {
    init_transport_tracing("doc_http=info", None, None, "info");

    let mut port: u16 = 4003;
    let mut host = "127.0.0.1".to_string();
    let mut repo_root: Option<PathBuf> = None;

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                port = args[i].parse().expect("invalid port");
            },
            "--host" => {
                i += 1;
                host = args[i].clone();
            },
            "--repo-root" => {
                i += 1;
                repo_root = Some(PathBuf::from(&args[i]));
            },
            _ => {},
        }
        i += 1;
    }

    let repo_root = repo_root
        .or_else(|| std::env::var("DOC_REPO_ROOT").ok().map(PathBuf::from))
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));

    let state = DocAppState::new(repo_root);
    let config = ServeConfig { host, port };

    if let Err(err) = start_server(config, state).await {
        eprintln!("Fatal error: {err}");
        std::process::exit(1);
    }
}
