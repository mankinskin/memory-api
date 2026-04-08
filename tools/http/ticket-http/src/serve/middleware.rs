//! Axum middleware for ticket-http write endpoints.
//!
//! `write_auth` gates mutation routes behind optional bearer-token authentication.
//! When `AppState::auth` is `None` (local/dev mode) all requests pass through.
//! When auth is configured a valid `Authorization: Bearer <token>` header is required.

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};

use viewer_api::{
    auth::extract_bearer_token,
    error::{ApiError, RequestIdExt},
};

use super::AppState;

/// Optional bearer-token gate applied to all write/mutation routes.
pub async fn write_auth(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(token_set) = &state.auth else {
        // No auth configured — pass through (local/dev mode).
        return next.run(request).await;
    };

    let request_id = request
        .extensions()
        .get::<RequestIdExt>()
        .map(|r| r.0.clone())
        .unwrap_or_default();

    match extract_bearer_token(request.headers()) {
        Some(token) if token_set.contains(token) => next.run(request).await,
        Some(_) => ApiError::unauthorized(
            "auth.invalid_token",
            "Bearer token is invalid",
            &request_id,
        )
        .into_response_with_status(StatusCode::UNAUTHORIZED),
        None => ApiError::unauthorized(
            "auth.missing_token",
            "Authorization header required",
            &request_id,
        )
        .into_response_with_status(StatusCode::UNAUTHORIZED),
    }
}
