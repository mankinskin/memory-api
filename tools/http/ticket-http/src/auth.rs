//! Bearer token authentication primitives used by the ticket HTTP server.

use std::{
    collections::HashSet,
    sync::Arc,
};

use axum::{
    body::Body,
    http::{
        HeaderMap,
        Request,
        StatusCode,
    },
    middleware::Next,
    response::Response,
};

use crate::error::{
    ApiError,
    RequestIdExt,
};

#[derive(Clone, Debug)]
pub struct TokenSet {
    tokens: HashSet<String>,
}

impl TokenSet {
    pub fn new(tokens: impl IntoIterator<Item = String>) -> Self {
        Self {
            tokens: tokens.into_iter().collect(),
        }
    }

    pub fn single(token: impl Into<String>) -> Self {
        let mut set = HashSet::new();
        set.insert(token.into());
        Self { tokens: set }
    }

    pub fn contains(
        &self,
        token: &str,
    ) -> bool {
        self.tokens.contains(token)
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

pub fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get("authorization")?.to_str().ok()?;
    value.strip_prefix("Bearer ").map(str::trim)
}

pub async fn bearer_auth_mw(
    axum::extract::State(token_set): axum::extract::State<Arc<TokenSet>>,
    request: Request<Body>,
    next: Next,
) -> Response {
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
