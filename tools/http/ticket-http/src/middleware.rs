//! Axum middleware utilities used by the ticket HTTP server.

pub mod request_id {
    use axum::{
        body::Body,
        http::Request,
        middleware::Next,
        response::Response,
    };
    use uuid::Uuid;

    use crate::error::RequestIdExt;

    pub async fn add_request_id(
        mut request: Request<Body>,
        next: Next,
    ) -> Response {
        let id = Uuid::new_v4().to_string();
        request.extensions_mut().insert(RequestIdExt(id.clone()));

        let mut response = next.run(request).await;

        if let Ok(value) = axum::http::HeaderValue::from_str(&id) {
            response.headers_mut().insert("x-request-id", value);
        }

        response
    }
}