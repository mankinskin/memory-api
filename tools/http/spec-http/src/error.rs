//! Error helpers for spec-http handlers.

use axum::http::StatusCode;
use axum::response::Response;
use viewer_api::error::ApiError;

pub use viewer_api::error::RequestIdExt;

/// Map a `SpecError` to an Axum `Response` with appropriate HTTP status.
pub fn spec_err(e: spec_api::error::SpecError, rid: &str) -> Response {
    use spec_api::error::SpecError;
    match e {
        SpecError::NotFound(_) => ApiError::not_found("spec", rid)
            .into_response_with_status(StatusCode::NOT_FOUND),
        SpecError::InvalidSlug(msg) => ApiError::new("spec.invalid_slug", &msg, rid)
            .into_response_with_status(StatusCode::BAD_REQUEST),
        SpecError::DuplicateSlug(slug) => {
            ApiError::new(
                "spec.duplicate_slug",
                &format!("slug already exists: {slug}"),
                rid,
            )
            .into_response_with_status(StatusCode::CONFLICT)
        }
        SpecError::Validation(e) => ApiError::new("spec.validation", &e.to_string(), rid)
            .into_response_with_status(StatusCode::UNPROCESSABLE_ENTITY),
        _ => {
            tracing::error!(error = %e, "spec error in http handler");
            ApiError::internal(rid).into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Map a `StorageError` to an Axum `Response`.
pub fn storage_err(e: memory_api::error::StorageError, rid: &str) -> Response {
    use memory_api::error::StorageError;
    match e {
        StorageError::NotFound(_) => ApiError::not_found("spec", rid)
            .into_response_with_status(StatusCode::NOT_FOUND),
        _ => {
            tracing::error!(error = %e, "storage error in spec-http handler");
            ApiError::internal(rid).into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
