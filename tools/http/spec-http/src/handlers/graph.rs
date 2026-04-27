//! Graph view: all specs as nodes, parent→child + shared-code-ref edges.

use std::collections::{BTreeMap, BTreeSet};

use axum::{
    extract::{Extension, State},
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;

use viewer_api::error::RequestIdExt;

use crate::error::storage_err;
use crate::state::SpecAppState;

#[derive(Serialize)]
pub struct GraphNode {
    pub id:        String,
    pub slug:      Option<String>,
    pub title:     Option<String>,
    pub state:     Option<String>,
    pub component: Option<String>,
}

#[derive(Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to:   String,
    /// One of: `"parent"` (parent → child in the spec tree) or
    /// `"code_ref"` (two specs share at least one referenced file).
    pub kind: String,
}

#[derive(Serialize)]
pub struct GraphResponse {
    pub request_id: String,
    pub nodes:      Vec<GraphNode>,
    pub edges:      Vec<GraphEdge>,
}

/// `GET /api/specs/graph` — full dependency graph of every spec.
pub async fn get_graph(
    State(state):     State<SpecAppState>,
    Extension(rid):   Extension<RequestIdExt>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);

    let all = match store.entity_store().list_indexed(false) {
        Ok(a)  => a,
        Err(e) => return storage_err(e, &rid.0),
    };

    // Read every spec manifest (skip soft-deleted / unreadable).
    let mut specs = Vec::with_capacity(all.len());
    for indexed in &all {
        if indexed.deleted { continue; }
        if let Ok(spec) = store.get(&indexed.id.to_string()) {
            specs.push(spec);
        }
    }

    // Build nodes.
    let nodes: Vec<GraphNode> = specs
        .iter()
        .map(|s| GraphNode {
            id:        s.id.to_string(),
            slug:      s.slug().map(str::to_string),
            title:     s.title().map(str::to_string),
            state:     s.state().map(str::to_string),
            component: s.component().map(str::to_string),
        })
        .collect();

    // Index spec ids that actually exist (so we don't emit dangling edges).
    let known: BTreeSet<String> = nodes.iter().map(|n| n.id.clone()).collect();

    let mut edges: Vec<GraphEdge> = Vec::new();

    // 1. parent → child edges from the spec tree.
    for spec in &specs {
        if let Some(parent_id) = spec.parent() {
            if known.contains(parent_id) {
                edges.push(GraphEdge {
                    from: parent_id.to_string(),
                    to:   spec.id.to_string(),
                    kind: "parent".to_string(),
                });
            }
        }
    }

    // 2. Code-ref overlap edges: file path → set of specs referencing it.
    let mut by_file: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let id_strings: Vec<String> = specs.iter().map(|s| s.id.to_string()).collect();
    for (i, spec) in specs.iter().enumerate() {
        for cr in &spec.code_refs {
            by_file.entry(cr.file.as_str())
                .or_default()
                .push(id_strings[i].as_str());
        }
    }
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for ids in by_file.values() {
        let unique: BTreeSet<&str> = ids.iter().copied().collect();
        if unique.len() < 2 { continue; }
        let v: Vec<&str> = unique.into_iter().collect();
        for i in 0..v.len() {
            for j in (i + 1)..v.len() {
                let (a, b) = (v[i].to_string(), v[j].to_string());
                let key = if a < b { (a.clone(), b.clone()) } else { (b.clone(), a.clone()) };
                if seen.insert(key.clone()) {
                    edges.push(GraphEdge {
                        from: key.0,
                        to:   key.1,
                        kind: "code_ref".to_string(),
                    });
                }
            }
        }
    }

    Json(GraphResponse { request_id: rid.0, nodes, edges }).into_response()
}
