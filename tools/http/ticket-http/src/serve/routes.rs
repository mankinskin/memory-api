//! Route table for `ticket serve`.

use axum::{
    Router,
    middleware,
    routing::get,
};

use viewer_api::middleware::request_id::add_request_id;

use super::{AppState, handlers};

/// Build the full Axum router.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(handlers::health::healthz))
        .route("/api/workspaces", get(handlers::workspaces::list_workspaces))
        .route("/api/tickets", get(handlers::tickets::list_tickets))
        .route("/api/tickets/{id}", get(handlers::tickets::get_ticket))
        .route("/api/tickets/{id}/description", get(handlers::tickets::get_ticket_description))
        .route("/api/edges", get(handlers::edges::list_edges))
        .route("/api/graph/subgraph", get(handlers::graph::subgraph))
        .route("/api/graph/topgraph", get(handlers::graph::topgraph))
        .route("/api/graph/health", get(handlers::graph::health_check))
        .route("/api/stream", get(handlers::stream::stream_handler))
        .layer(middleware::from_fn(add_request_id))
        .with_state(state)
}
