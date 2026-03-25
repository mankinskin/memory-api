//! HTTP serve mode for `ticket serve`.
//!
//! Exposes the ticket store over a REST/SSE API with bearer-token auth.
//! See `api-contract-v0.1.md` (ticket `21a1b9ca`) for the full endpoint spec.

pub mod auth_state;
pub mod error;
mod handlers;
pub mod registry;
pub mod routes;
pub mod stream;

use std::{net::SocketAddr, sync::Arc};
use std::{collections::HashSet, sync::Mutex};

use tokio::net::TcpListener;

pub use auth_state::AuthState;
pub use registry::WorkspaceRegistry;
pub use stream::{HookEmitter, StreamBroker};
use ticket_api::storage::store::TicketStore;

/// Configuration for `ticket serve`.
#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

impl ServeConfig {
    pub fn addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("valid socket address")
    }
}

/// Shared application state passed to all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<WorkspaceRegistry>,
    pub broker: Arc<StreamBroker>,
    runtime_ready: Arc<Mutex<HashSet<String>>>,
}

impl AppState {
    pub fn new(
        registry: Arc<WorkspaceRegistry>,
        broker: Arc<StreamBroker>,
    ) -> Self {
        Self {
            registry,
            broker,
            runtime_ready: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Get a workspace store and ensure serve-runtime wiring is initialized once.
    ///
    /// This guarantees that even lazily opened workspaces get a broker channel,
    /// store hook emitter, and reconcile loop.
    pub fn ensure_workspace_runtime(
        &self,
        workspace: &str,
    ) -> Option<Arc<TicketStore>> {
        let store = self.registry.get(workspace)?;

        let mut ready = self.runtime_ready.lock().unwrap();
        if !ready.insert(workspace.to_string()) {
            return Some(store);
        }

        self.broker.ensure_channel(workspace);
        let emitter = HookEmitter::new(workspace.to_string(), Arc::clone(&self.broker));
        store.set_hook(emitter.clone());
        stream::reconcile::spawn_reconcile(Arc::clone(&store), emitter);

        Some(store)
    }
}

/// Start the HTTP server.
///
/// Creates a `StreamBroker`, wires up per-workspace `HookEmitter`s and
/// reconcile loops, then starts Axum.
pub async fn serve(
    config: ServeConfig,
    registry: WorkspaceRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState::new(
        Arc::new(registry),
        Arc::new(StreamBroker::new()),
    );

    // Pre-initialize all known workspaces at startup while still keeping
    // on-demand initialization for lazily opened stores.
    let workspace_names = state.registry.workspace_names();
    for ws in &workspace_names {
        let _ = state.ensure_workspace_runtime(ws);
    }

    let app = routes::build_router(state);
    let addr = config.addr();
    let listener = TcpListener::bind(addr).await?;

    eprintln!("ticket serve listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AppState, StreamBroker, WorkspaceRegistry};
    use ticket_api::model::filesystem::ScanRoot;
    use crate::serve::stream::event::SseEvent;
    use std::{collections::BTreeMap, sync::Arc};

    #[tokio::test]
    async fn ensure_workspace_runtime_wires_hook_for_lazy_open_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(
            Arc::new(WorkspaceRegistry::single(dir.path().to_path_buf())),
            Arc::new(StreamBroker::new()),
        );

        let mut rx = state.broker.subscribe("default");
        let store = state
            .ensure_workspace_runtime("default")
            .expect("workspace should initialize");

        store
            .add_scan_root(ScanRoot {
                path: dir.path().join("tickets"),
                label: "default".to_string(),
            })
            .expect("add scan root");

        store
            .create(
                None,
                "tracker-improvement",
                Some("runtime wiring regression"),
                Some("open"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create ticket");

        let (_, event) = rx.recv().await.expect("should receive hook event");
        match event {
            SseEvent::TicketUpsert(payload) => {
                assert_eq!(payload.workspace, "default");
                assert_eq!(payload.ticket.title.as_deref(), Some("runtime wiring regression"));
            }
            other => panic!("expected TicketUpsert, got {other:?}"),
        }
    }
}
