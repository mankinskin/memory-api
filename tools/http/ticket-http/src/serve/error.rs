//! Error helpers re-exported from viewer_api for use in serve handlers.

use std::io::ErrorKind;

use axum::{
    http::StatusCode,
    response::Response,
};
use serde_json::json;
pub use viewer_api::error::{
    ApiError,
    RequestIdExt,
};

/// Helper: extract request_id from extensions or fall back to empty string.
pub fn request_id(
    ext: Option<axum::extract::Extension<RequestIdExt>>
) -> String {
    ext.map(|e| e.0.0.clone()).unwrap_or_default()
}

/// Map a `StorageError` to an Axum Response.
///
/// Every arm logs an appropriately-levelled tracing event so that errors are
/// captured in the server log even when the HTTP client ignores the body.
/// The `rid` (request-id) field is included in every event so log lines can be
/// correlated with client-side `ApiRequestError` reports.
pub fn storage_err(
    e: ticket_api::error::StorageError,
    rid: &str,
) -> Response {
    use ticket_api::error::StorageError;
    match e {
        StorageError::NotFound(id) => {
            tracing::debug!(request_id = %rid, ticket_id = %id, "ticket not found");
            ApiError::new(
                "ticket.not_found",
                format!("ticket {id} was not found"),
                rid,
            )
            .into_response_with_status(StatusCode::NOT_FOUND)
        },
        StorageError::Io(error) if error.kind() == ErrorKind::NotFound => {
            tracing::warn!(
                request_id = %rid,
                io_error = %error,
                "ticket data missing from disk"
            );
            ApiError::new(
                "storage.path_not_found",
                "ticket data is missing from disk for the requested workspace",
                rid,
            )
            .with_details(json!({ "error": error.to_string() }))
            .into_response_with_status(StatusCode::NOT_FOUND)
        },
        StorageError::Validation(error) => {
            tracing::debug!(request_id = %rid, error = %error, "ticket validation failed");
            ApiError::new(
                "ticket.validation_failed",
                error.to_string(),
                rid,
            )
            .into_response_with_status(StatusCode::UNPROCESSABLE_ENTITY)
        },
        StorageError::QueryParse(error) => {
            tracing::debug!(request_id = %rid, error = %error, "query parse error");
            ApiError::bad_request(
                "query.invalid",
                error.to_string(),
                rid,
            )
            .into_response_with_status(StatusCode::BAD_REQUEST)
        },
        StorageError::LeaseConflict { ticket, holder } => {
            tracing::warn!(
                request_id = %rid,
                ticket_id = %ticket,
                holder = %holder,
                "lease conflict"
            );
            ApiError::conflict(
                "ticket.lease_conflict",
                format!("ticket {ticket} is currently held by {holder}"),
                rid,
            )
            .with_details(json!({
                "ticket": ticket.to_string(),
                "holder": holder,
            }))
            .into_response_with_status(StatusCode::CONFLICT)
        },
        StorageError::DependencyCycle => {
            tracing::debug!(request_id = %rid, "dependency cycle detected");
            ApiError::new(
                "edge.cycle_detected",
                "Adding this edge would create a dependency cycle",
                rid,
            )
            .into_response_with_status(StatusCode::UNPROCESSABLE_ENTITY)
        },
        StorageError::SchemaMismatch(error) => {
            tracing::warn!(request_id = %rid, error = %error, "schema mismatch");
            ApiError::new(
                "storage.schema_mismatch",
                error.to_string(),
                rid,
            )
            .into_response_with_status(StatusCode::CONFLICT)
        },
        StorageError::ParseError { path, reason } => {
            tracing::error!(
                request_id = %rid,
                path = %path.display(),
                reason = %reason,
                "failed to parse ticket data"
            );
            ApiError::new(
                "storage.parse_error",
                format!("failed to parse ticket data: {reason}"),
                rid,
            )
            .with_details(json!({ "path": path.display().to_string() }))
            .into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
        },
        StorageError::SchemaFileParse { path, reason } => {
            tracing::error!(
                request_id = %rid,
                path = %path.display(),
                reason = %reason,
                "failed to parse schema file"
            );
            ApiError::new(
                "storage.schema_file_parse_error",
                format!("failed to parse schema file: {reason}"),
                rid,
            )
            .with_details(json!({ "path": path.display().to_string() }))
            .into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
        },
        StorageError::Protocol(error) => {
            tracing::warn!(request_id = %rid, error = %error, "protocol error");
            ApiError::new(
                error.code(),
                error.to_string(),
                rid,
            )
            .into_response_with_status(StatusCode::UNPROCESSABLE_ENTITY)
        },
        StorageError::Database(message) => {
            tracing::error!(request_id = %rid, message = %message, "ticket database error");
            ApiError::new(
                "storage.database_error",
                format!("ticket database error: {message}"),
                rid,
            )
            .into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
        },
        StorageError::SearchIndex(message) => {
            tracing::error!(
                request_id = %rid,
                message = %message,
                "ticket search index error"
            );
            ApiError::new(
                "storage.search_index_error",
                format!("ticket search index error: {message}"),
                rid,
            )
            .into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
        },
        // ── This is the "ticket serialization error: io error unexpected end of
        //    file" case.  We log at ERROR with full detail so it is always
        //    visible regardless of the configured log level filter.
        StorageError::Serialization(message) => {
            tracing::error!(
                request_id = %rid,
                message = %message,
                "ticket serialization error — a ticket file may be truncated or \
                 written non-atomically; check the .ticket directory for zero-byte \
                 or partial TOML files"
            );
            ApiError::new(
                "storage.serialization_error",
                format!("ticket serialization error: {message}"),
                rid,
            )
            .into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
        },
        StorageError::Io(error) => {
            tracing::error!(
                request_id = %rid,
                io_error = %error,
                kind = ?error.kind(),
                "ticket storage I/O error"
            );
            ApiError::new(
                "storage.io_error",
                format!("ticket storage I/O error: {error}"),
                rid,
            )
            .into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
        },
        StorageError::Other(message) => {
            tracing::error!(request_id = %rid, message = %message, "ticket storage error");
            ApiError::new(
                "storage.error",
                message,
                rid,
            )
            .into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
        },
        StorageError::WorkspaceNotFound { path } => {
            tracing::warn!(
                request_id = %rid,
                path = %path.display(),
                "workspace not initialized"
            );
            ApiError::new(
                "workspace.not_initialized",
                format!(
                    "no ticket workspace found at {}; run 'ticket init' to create one",
                    path.display()
                ),
                rid,
            )
            .into_response_with_status(StatusCode::SERVICE_UNAVAILABLE)
        },
    }
}

pub fn task_join_err(
    rid: &str,
    operation: &str,
) -> Response {
    tracing::error!(request_id = %rid, operation, "ticket-http worker task aborted");
    ApiError::new(
        "internal.task_failed",
        format!(
            "the ticket backend aborted while processing {operation}; retry once and include request_id if the failure persists"
        ),
        rid,
    )
    .into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
}


#[cfg(test)]
mod tests {
    use super::storage_err;
    use axum::{
        body::to_bytes,
        http::StatusCode,
    };
    use ticket_api::error::StorageError;

    #[tokio::test]
    async fn io_not_found_maps_to_actionable_404() {
        let response = storage_err(
            StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "missing ticket.toml",
            )),
            "rid-test",
        );

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("json body");
        assert_eq!(payload["code"], "storage.path_not_found");
        assert!(
            payload["message"]
                .as_str()
                .expect("message")
                .contains("missing from disk")
        );
    }

    #[tokio::test]
    async fn other_storage_errors_keep_specific_message() {
        let response = storage_err(
            StorageError::Other("workspace index is inconsistent".to_string()),
            "rid-test",
        );

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("json body");
        assert_eq!(payload["code"], "storage.error");
        assert_eq!(payload["message"], "workspace index is inconsistent");
    }
}
