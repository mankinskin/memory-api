//! Route table for spec-http.

use axum::{
    Router,
    middleware,
    routing::{delete, get, patch, post},
};
use tower_http::cors::{Any, CorsLayer};
use viewer_api::middleware::request_id::add_request_id;

use crate::handlers;
use crate::state::SpecAppState;

/// Build the full Axum router.
pub fn build_router(state: SpecAppState) -> Router {
    let read_routes = Router::new()
        .route("/healthz", get(handlers::health::healthz))
        .route("/api/specs", get(handlers::specs::list_specs))
        .route("/api/specs/search", get(handlers::specs::search_specs))
        .route("/api/specs/graph", get(handlers::graph::get_graph))
        .route("/api/specs/health", get(handlers::health::health_check))
        .route("/api/specs/stream", get(handlers::stream::spec_stream))
        .route("/api/specs/{id}", get(handlers::specs::get_spec))
        .route("/api/specs/{id}/full", get(handlers::specs::get_spec_full))
        .route("/api/specs/{id}/tree", get(handlers::tree::get_tree))
        .route("/api/specs/{id}/refs", get(handlers::tree::get_refs))
        .route(
            "/api/specs/{id}/sections",
            get(handlers::sections::list_sections),
        )
        .route(
            "/api/specs/{id}/sections/{name}",
            get(handlers::sections::get_section),
        );

    let write_routes = Router::new()
        .route("/api/specs", post(handlers::specs::create_spec))
        .route(
            "/api/specs/{id}",
            patch(handlers::specs::update_spec).delete(handlers::specs::delete_spec),
        )
        .route(
            "/api/specs/{id}/refs/validate",
            post(handlers::tree::validate_refs),
        )
        .route(
            "/api/specs/{id}/sections",
            post(handlers::sections::add_section),
        )
        .route(
            "/api/specs/{id}/sections/{name}",
            delete(handlers::sections::delete_section),
        )
        .route("/api/specs/scan", post(handlers::health::scan))
        .route("/api/specs/add-root", post(handlers::health::add_root));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    read_routes
        .merge(write_routes)
        .layer(cors)
        .layer(middleware::from_fn(add_request_id))
        .with_state(state)
}
