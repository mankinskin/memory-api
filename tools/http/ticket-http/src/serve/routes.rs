//! Route table for `ticket serve`.

use axum::{
    Router,
    middleware,
    routing::{delete, get, patch, post},
};

use viewer_api::middleware::request_id::add_request_id;

use super::{AppState, handlers, middleware as mw};

/// Build the full Axum router.
pub fn build_router(state: AppState) -> Router {
    let read_routes = Router::new()
        .route("/healthz", get(handlers::health::healthz))
        .route("/api/workspaces", get(handlers::workspaces::list_workspaces))
        .route("/api/tickets", get(handlers::tickets::list_tickets))
        .route("/api/tickets/{id}", get(handlers::tickets::get_ticket))
        .route("/api/tickets/{id}/description", get(handlers::tickets::get_ticket_description))
        .route("/api/tickets/{id}/history", get(handlers::tickets::get_ticket_history))
        .route("/api/edges", get(handlers::edges::list_edges))
        .route("/api/schema", get(handlers::schema::list_schemas))
        .route("/api/schema/{type_id}", get(handlers::schema::get_schema))
        .route("/api/graph/subgraph", get(handlers::graph::subgraph))
        .route("/api/graph/topgraph", get(handlers::graph::topgraph))
        .route("/api/graph/health", get(handlers::graph::health_check))
        .route("/api/stream", get(handlers::stream::stream_handler));

    let write_routes = Router::new()
        .route("/api/tickets", post(handlers::tickets::create_ticket))
        .route(
            "/api/tickets/{id}",
            patch(handlers::tickets::update_ticket).delete(handlers::tickets::delete_ticket),
        )
        .route("/api/tickets/{id}/close", post(handlers::tickets::close_ticket))
        .route("/api/tickets/{id}/cancel", post(handlers::tickets::cancel_ticket))
        .route(
            "/api/edges",
            post(handlers::edges::add_edge).delete(handlers::edges::remove_edge),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), mw::write_auth));

    read_routes
        .merge(write_routes)
        .layer(middleware::from_fn(add_request_id))
        .with_state(state)
}
