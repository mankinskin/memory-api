pub mod error;
pub mod handlers;
pub mod routes;
pub mod state;

pub use routes::build_router;
pub use state::DocAppState;

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

pub async fn start_server(
    config: ServeConfig,
    state: DocAppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = build_router(state);
    let addr = config.addr();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("doc-http listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
