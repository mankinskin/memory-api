use std::path::PathBuf;

use axum::{
    Json,
    http::StatusCode,
    response::{
        IntoResponse,
        Response,
    },
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocHttpError {
    #[error(transparent)]
    Doc(#[from] doc_api::DocError),

    #[error("package not found: {0}")]
    PackageNotFound(String),

    #[error(
        "artifact not found for package '{package}', target '{target}', kind '{kind}'"
    )]
    ArtifactNotFound {
        package: String,
        target: String,
        kind: &'static str,
    },

    #[error("failed to read artifact {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl DocHttpError {
    pub fn io(
        path: PathBuf,
        source: std::io::Error,
    ) -> Self {
        Self::Io { path, source }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Doc(_) => "doc.workspace_error",
            Self::PackageNotFound(_) => "doc.package_not_found",
            Self::ArtifactNotFound { .. } => "doc.artifact_not_found",
            Self::Io { .. } => "doc.artifact_read_failed",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::PackageNotFound(_) | Self::ArtifactNotFound { .. } =>
                StatusCode::NOT_FOUND,
            Self::Doc(_) | Self::Io { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for DocHttpError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = Json(ApiError {
            code: self.code(),
            message: self.to_string(),
        });
        (status, body).into_response()
    }
}
