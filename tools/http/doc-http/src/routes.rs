use axum::{
    Router,
    routing::get,
};
use tower_http::cors::{
    Any,
    CorsLayer,
};

use crate::{
    handlers,
    state::DocAppState,
};

pub fn build_router(state: DocAppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/healthz", get(handlers::healthz))
        .route("/api/docs/workspace", get(handlers::get_workspace))
        .route("/api/docs/artifacts", get(handlers::list_artifacts))
        .route("/api/docs/artifacts/{package}", get(handlers::get_package_artifacts))
        .route(
            "/api/docs/artifacts/{package}/{target}/html",
            get(handlers::get_html_artifact),
        )
        .route(
            "/api/docs/artifacts/{package}/{target}/rustdoc-json",
            get(handlers::get_rustdoc_json_artifact),
        )
        .layer(cors)
        .with_state(state)
}
