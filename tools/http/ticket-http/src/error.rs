//! Structured API error envelope matching the ticket HTTP error shape.

use axum::{
    http::StatusCode,
    response::{
        IntoResponse,
        Json,
        Response,
    },
};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Default)]
pub struct RequestIdExt(pub String);

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ApiError {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        request_id: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            request_id: request_id.into(),
            details: None,
        }
    }

    pub fn with_details(
        mut self,
        details: Value,
    ) -> Self {
        self.details = Some(details);
        self
    }

    pub fn unauthorized(
        code: impl Into<String>,
        message: impl Into<String>,
        request_id: &str,
    ) -> Self {
        Self::new(code, message, request_id)
    }

    pub fn not_found(
        resource: impl Into<String>,
        request_id: &str,
    ) -> Self {
        let resource = resource.into();
        Self::new("not_found", format!("{resource} not found"), request_id)
    }

    pub fn bad_request(
        code: impl Into<String>,
        message: impl Into<String>,
        request_id: &str,
    ) -> Self {
        Self::new(code, message, request_id)
    }

    pub fn internal(request_id: &str) -> Self {
        Self::new("internal_error", "An unexpected error occurred", request_id)
    }

    pub fn conflict(
        code: impl Into<String>,
        message: impl Into<String>,
        request_id: &str,
    ) -> Self {
        Self::new(code, message, request_id)
    }

    pub fn into_response_with_status(
        self,
        status: StatusCode,
    ) -> Response {
        (status, Json(self)).into_response()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        self.into_response_with_status(StatusCode::INTERNAL_SERVER_ERROR)
    }
}
