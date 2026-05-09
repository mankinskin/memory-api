//! Graph view: all specs as nodes, parent->child + shared-code-ref edges.

use std::collections::{BTreeMap, BTreeSet};

use axum::{
    extract::{Extension, State},
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use spec_api::{SpecManifest, SpecStore};

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
    /// One of: `"parent"` (parent -> child in the spec tree) or
    /// `"code_ref"` (two specs share at least one referenced file).
    pub kind: String,
}

#[derive(Serialize)]
pub struct GraphResponse {
    pub request_id: String,
    pub nodes:      Vec<GraphNode>,
    pub edges:      Vec<GraphEdge>,
}

/// `GET /api/specs/graph` - full dependency graph of every spec.
pub async fn get_graph(
    State(state):   State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);

    let specs = match load_specs(&mut store, &rid.0) {
        Ok(specs) => specs,
        Err(response) => return response,
    };

    let nodes = build_nodes(&specs);
    let edges = build_edges(&specs, &nodes);

    Json(GraphResponse {
        request_id: rid.0,
        nodes,
        edges,
    })
    .into_response()
}

fn load_specs(store: &mut SpecStore, request_id: &str) -> Result<Vec<SpecManifest>, Response> {
    let all = match store.entity_store().list_indexed(false) {
        Ok(all) => all,
        Err(err) => return Err(storage_err(err, request_id)),
    };

    let mut specs = Vec::with_capacity(all.len());
    for indexed in &all {
        if indexed.deleted {
            continue;
        }
        if let Ok(spec) = store.get(&indexed.id.to_string()) {
            specs.push(spec);
        }
    }

    Ok(specs)
}

fn build_nodes(specs: &[SpecManifest]) -> Vec<GraphNode> {
    specs
        .iter()
        .map(|spec| GraphNode {
            id:        spec.id.to_string(),
            slug:      spec.slug().map(str::to_string),
            title:     spec.title().map(str::to_string),
            state:     spec.state().map(str::to_string),
            component: spec.component().map(str::to_string),
        })
        .collect()
}

fn build_edges(specs: &[SpecManifest], nodes: &[GraphNode]) -> Vec<GraphEdge> {
    let known: BTreeSet<String> = nodes.iter().map(|node| node.id.clone()).collect();
    let mut edges = parent_edges(specs, &known);
    edges.extend(code_ref_edges(specs));
    edges
}

fn parent_edges(specs: &[SpecManifest], known: &BTreeSet<String>) -> Vec<GraphEdge> {
    let mut edges = Vec::new();
    for spec in specs {
        let Some(parent_id) = spec.parent() else {
            continue;
        };
        if known.contains(parent_id) {
            edges.push(GraphEdge {
                from: parent_id.to_string(),
                to:   spec.id.to_string(),
                kind: "parent".to_string(),
            });
        }
    }
    edges
}

fn code_ref_edges(specs: &[SpecManifest]) -> Vec<GraphEdge> {
    let mut by_file: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let id_strings: Vec<String> = specs.iter().map(|spec| spec.id.to_string()).collect();

    for (index, spec) in specs.iter().enumerate() {
        for code_ref in &spec.code_refs {
            by_file
                .entry(code_ref.file.as_str())
                .or_default()
                .push(id_strings[index].as_str());
        }
    }

    let mut edges = Vec::new();
    let mut seen = BTreeSet::new();
    for ids in by_file.values() {
        let unique: BTreeSet<&str> = ids.iter().copied().collect();
        if unique.len() < 2 {
            continue;
        }

        let ordered: Vec<&str> = unique.into_iter().collect();
        for left in 0..ordered.len() {
            for right in (left + 1)..ordered.len() {
                let a = ordered[left].to_string();
                let b = ordered[right].to_string();
                let key = if a < b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                };

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

    edges
}
