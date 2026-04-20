pub mod error;
pub mod handlers;
pub mod routes;
pub mod state;

pub use routes::build_router;
pub use state::SpecAppState;

/// Configuration for the HTTP server.
#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub host: String,
    pub port: u16,
}

impl ServeConfig {
    pub fn addr(&self) -> std::net::SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("valid address")
    }
}

/// Start the spec-http server.
pub async fn start_server(
    config: ServeConfig,
    state: SpecAppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Auto-scan on startup so slugs are available
    {
        let mut store = state.store.lock().await;
        let _ = store.scan(false);
    }

    let app = build_router(state);
    let addr = config.addr();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("spec-http listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
