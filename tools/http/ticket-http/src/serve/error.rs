//! Error helpers re-exported from viewer_api for use in serve handlers.

pub use viewer_api::error::{ApiError, RequestIdExt};
use axum::http::StatusCode;
use axum::response::Response;

/// Helper: extract request_id from extensions or fall back to empty string.
pub fn request_id(ext: Option<axum::extract::Extension<RequestIdExt>>) -> String {
    ext.map(|e| e.0.0.clone()).unwrap_or_default()
}

/// Map a `StorageError` to an Axum Response.
pub fn storage_err(
    e: ticket_api::error::StorageError,
    rid: &str,
) -> Response {
    use ticket_api::error::StorageError;
    match e {
        StorageError::NotFound(_) => {
            ApiError::not_found("ticket", rid)
                .into_response_with_status(StatusCode::NOT_FOUND)
        }
        StorageError::DependencyCycle => {
            ApiError::new(
                "edge.cycle_detected",
                "Adding this edge would create a dependency cycle",
                rid,
            )
            .into_response_with_status(StatusCode::UNPROCESSABLE_ENTITY)
        }
        _ => {
            tracing::error!(error = %e, "storage error in serve handler");
            ApiError::internal(rid)
                .into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
