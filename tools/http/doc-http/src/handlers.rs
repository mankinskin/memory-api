use axum::{
    Json,
    extract::{
        Path,
        State,
    },
    http::header,
    response::{
        Html,
        IntoResponse,
    },
};
use doc_api::CargoDocArtifact;
use serde::Serialize;

use crate::{
    error::DocHttpError,
    state::DocAppState,
};

#[derive(Debug, Serialize)]
pub struct WorkspaceResponse {
    pub workspace_root: std::path::PathBuf,
    pub workspace_manifest_path: std::path::PathBuf,
    pub target_directory: std::path::PathBuf,
    pub package_count: usize,
    pub packages: Vec<PackageSummary>,
}

#[derive(Debug, Serialize)]
pub struct PackageSummary {
    pub name: String,
    pub version: String,
    pub package_root: std::path::PathBuf,
    pub target_count: usize,
    pub doc_target_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ArtifactListResponse {
    pub workspace_root: std::path::PathBuf,
    pub target_directory: std::path::PathBuf,
    pub artifact_count: usize,
    pub artifacts: Vec<CargoDocArtifact>,
}

#[derive(Debug, Serialize)]
pub struct PackageArtifactResponse {
    pub package_name: String,
    pub artifact_count: usize,
    pub artifacts: Vec<CargoDocArtifact>,
}

pub async fn healthz() -> &'static str {
    "ok"
}

pub async fn get_workspace(
    State(state): State<DocAppState>
) -> Result<Json<WorkspaceResponse>, DocHttpError> {
    let workspace = state.load_workspace()?;
    let packages = workspace
        .packages
        .iter()
        .map(|package| PackageSummary {
            name: package.name.clone(),
            version: package.version.clone(),
            package_root: package.package_root.clone(),
            target_count: package.targets.len(),
            doc_target_count: package
                .targets
                .iter()
                .filter(|target| target.doc_capable)
                .count(),
        })
        .collect::<Vec<_>>();

    Ok(Json(WorkspaceResponse {
        workspace_root: workspace.workspace_root,
        workspace_manifest_path: workspace.workspace_manifest_path,
        target_directory: workspace.target_directory,
        package_count: packages.len(),
        packages,
    }))
}

pub async fn list_artifacts(
    State(state): State<DocAppState>
) -> Result<Json<ArtifactListResponse>, DocHttpError> {
    let workspace = state.load_workspace()?;
    let artifacts = workspace.cargo_doc_artifacts();

    Ok(Json(ArtifactListResponse {
        workspace_root: workspace.workspace_root,
        target_directory: workspace.target_directory,
        artifact_count: artifacts.len(),
        artifacts,
    }))
}

pub async fn get_package_artifacts(
    Path(package_name): Path<String>,
    State(state): State<DocAppState>,
) -> Result<Json<PackageArtifactResponse>, DocHttpError> {
    let workspace = state.load_workspace()?;
    let package = workspace
        .package(&package_name)
        .ok_or_else(|| DocHttpError::PackageNotFound(package_name.clone()))?;
    let artifacts = package.cargo_doc_artifacts(&workspace.cargo_doc_root());

    Ok(Json(PackageArtifactResponse {
        package_name,
        artifact_count: artifacts.len(),
        artifacts,
    }))
}

pub async fn get_html_artifact(
    Path((package_name, target_name)): Path<(String, String)>,
    State(state): State<DocAppState>,
) -> Result<Html<String>, DocHttpError> {
    let artifact = find_artifact(&state, &package_name, &target_name)?;
    if !artifact.html_exists {
        return Err(DocHttpError::ArtifactNotFound {
            package: package_name,
            target: target_name,
            kind: "html",
        });
    }
    let html = tokio::fs::read_to_string(&artifact.html_index_path)
        .await
        .map_err(|source| {
            DocHttpError::io(artifact.html_index_path.clone(), source)
        })?;
    Ok(Html(html))
}

pub async fn get_rustdoc_json_artifact(
    Path((package_name, target_name)): Path<(String, String)>,
    State(state): State<DocAppState>,
) -> Result<impl IntoResponse, DocHttpError> {
    let artifact = find_artifact(&state, &package_name, &target_name)?;
    if !artifact.rustdoc_json_exists {
        return Err(DocHttpError::ArtifactNotFound {
            package: package_name,
            target: target_name,
            kind: "rustdoc-json",
        });
    }
    let json = tokio::fs::read_to_string(&artifact.rustdoc_json_path)
        .await
        .map_err(|source| {
            DocHttpError::io(artifact.rustdoc_json_path.clone(), source)
        })?;
    Ok(([(header::CONTENT_TYPE, "application/json")], json))
}

fn find_artifact(
    state: &DocAppState,
    package_name: &str,
    target_name: &str,
) -> Result<CargoDocArtifact, DocHttpError> {
    let workspace = state.load_workspace()?;
    workspace
        .cargo_doc_artifacts()
        .into_iter()
        .find(|artifact| {
            artifact.package_name == package_name
                && artifact.target_name == target_name
        })
        .ok_or_else(|| DocHttpError::ArtifactNotFound {
            package: package_name.to_string(),
            target: target_name.to_string(),
            kind: "metadata",
        })
}
