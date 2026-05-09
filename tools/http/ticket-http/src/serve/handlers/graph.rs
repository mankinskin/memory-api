use axum::{
    extract::{Extension, Query, State},
    response::Response,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

use viewer_api::error::RequestIdExt;

use crate::serve::AppState;

mod quality;
mod traversal;

#[derive(Deserialize)]
pub struct SubgraphQuery {
    pub workspace: String,
    pub root: Uuid,
    pub direction: Option<String>,
    pub edge_kind: Option<String>,
    #[serde(default = "default_depth")]
    pub depth: usize,
    #[serde(default = "default_limit_nodes")]
    pub limit_nodes: usize,
    #[serde(default = "default_limit_edges")]
    pub limit_edges: usize,
}

fn default_depth() -> usize { 2 }
fn default_limit_nodes() -> usize { 500 }
fn default_limit_edges() -> usize { 2000 }

#[derive(Serialize)]
pub struct NodeItem {
    pub id: String,
    pub title: Option<String>,
    pub state: Option<String>,
    pub depth: usize,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub ticket_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
}

#[derive(Serialize)]
pub struct EdgeItem {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Serialize)]
pub struct SubgraphStats {
    pub nodes_returned: usize,
    pub edges_returned: usize,
    pub max_depth_reached: usize,
    pub phase1_edges_ms: u128,
    pub phase2_bfs_ms: u128,
    pub phase3_meta_ms: u128,
    pub total_ms: u128,
}

#[derive(Serialize)]
pub struct SubgraphResponse {
    pub request_id: String,
    pub workspace: String,
    pub nodes: Vec<NodeItem>,
    pub edges: Vec<EdgeItem>,
    pub truncated: bool,
    pub next_cursor: Option<String>,
    pub stats: SubgraphStats,
}

pub async fn subgraph(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<SubgraphQuery>,
) -> Response {
    traversal::handle_subgraph(state, rid.0, params).await
}

#[derive(Deserialize)]
pub struct TopgraphQuery {
    pub workspace: String,
    pub root: Uuid,
    pub direction: Option<String>,
    pub edge_kind: Option<String>,
    #[serde(default = "default_depth")]
    pub depth: usize,
    #[serde(default = "default_limit_nodes")]
    pub limit_nodes: usize,
    #[serde(default = "default_limit_edges")]
    pub limit_edges: usize,
}

pub async fn topgraph(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<TopgraphQuery>,
) -> Response {
    traversal::handle_topgraph(state, rid.0, params).await
}

// ── Health check endpoint ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct HealthCheckQuery {
    pub workspace: String,
    pub root: Option<Uuid>,
    #[serde(default)]
    pub all: Option<bool>,
    #[serde(default = "default_health_depth")]
    pub depth: usize,
    pub direction: Option<String>,
}

fn default_health_depth() -> usize { 6 }

#[derive(Serialize)]
pub struct HealthCheckResponse {
    pub request_id: String,
    pub workspace: String,
    pub tickets_checked: usize,
    pub finding_count: usize,
    pub summary: BTreeMap<String, u64>,
    pub findings: Vec<serde_json::Value>,
}

pub async fn health_check(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<HealthCheckQuery>,
) -> Response {
    quality::handle_health_check(state, rid.0, params).await
}
