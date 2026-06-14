//! Graph view: all specs as nodes, parent->child + shared-code-ref edges.

use std::collections::{
    BTreeMap,
    BTreeSet,
};

use axum::{
    extract::{
        Extension,
        State,
    },
    response::{
        IntoResponse,
        Json,
        Response,
    },
};
use serde::Serialize;
use spec_api::{
    SpecManifest,
    SpecStore,
};

use viewer_api::error::RequestIdExt;

use crate::{
    error::storage_err,
    state::SpecAppState,
};

#[derive(Serialize)]
pub struct GraphNodeMetrics {
    pub child_count: usize,
    pub code_ref_count: usize,
    pub section_count: usize,
}

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub slug: Option<String>,
    pub title: Option<String>,
    pub state: Option<String>,
    pub component: Option<String>,
    pub scope: Option<String>,
    pub summary: Option<String>,
    pub summary_markdown: Option<String>,
    pub metrics: GraphNodeMetrics,
}

#[derive(Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    /// One of: `"parent"` (parent -> child in the spec tree) or
    /// `"code_ref"` (two specs share at least one referenced file).
    pub kind: String,
}

#[derive(Serialize)]
pub struct GraphResponse {
    pub request_id: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// `GET /api/specs/graph` - full dependency graph of every spec.
pub async fn get_graph(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);

    let specs = match load_specs(&mut store, &rid.0) {
        Ok(specs) => specs,
        Err(response) => return response,
    };

    let nodes = build_nodes(&mut store, &specs);
    let edges = build_edges(&specs, &nodes);

    Json(GraphResponse {
        request_id: rid.0,
        nodes,
        edges,
    })
    .into_response()
}

fn load_specs(
    store: &mut SpecStore,
    request_id: &str,
) -> Result<Vec<SpecManifest>, Response> {
    let all = match store.entity_store().list_indexed() {
        Ok(all) => all,
        Err(err) => return Err(storage_err(err, request_id)),
    };

    let mut specs = Vec::with_capacity(all.len());
    for indexed in &all {
        if let Ok(spec) = store.get(&indexed.id.to_string()) {
            specs.push(spec);
        }
    }

    Ok(specs)
}

fn build_nodes(
    store: &mut SpecStore,
    specs: &[SpecManifest],
) -> Vec<GraphNode> {
    let child_counts = count_children(specs);

    specs
        .iter()
        .map(|spec| {
            let id = spec.id.to_string();
            let section_count = store
                .list_sections(&id)
                .map(|sections| sections.len())
                .unwrap_or(0);
            let (summary, summary_markdown) = store
                .get_full(&id)
                .ok()
                .map(|(_, body)| {
                    (summarize_body(&body), summarize_body_markdown(&body))
                })
                .unwrap_or((None, None));

            GraphNode {
                id: id.clone(),
                slug: spec.slug().map(str::to_string),
                title: spec.title().map(str::to_string),
                state: spec.state().map(str::to_string),
                component: spec.component().map(str::to_string),
                scope: spec.scope().map(str::to_string),
                summary,
                summary_markdown,
                metrics: GraphNodeMetrics {
                    child_count: child_counts.get(&id).copied().unwrap_or(0),
                    code_ref_count: spec.code_refs.len(),
                    section_count,
                },
            }
        })
        .collect()
}

fn count_children(specs: &[SpecManifest]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for spec in specs {
        if let Some(parent_id) = spec.parent() {
            *counts.entry(parent_id.to_string()).or_insert(0) += 1;
        }
    }
    counts
}

fn first_meaningful_block(body: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut saw_content = false;

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            if saw_content {
                break;
            }
            continue;
        }
        if !saw_content && line.starts_with('#') {
            continue;
        }

        saw_content = true;
        lines.push(line.to_string());
    }

    lines
}

fn summarize_body(body: &str) -> Option<String> {
    let lines = first_meaningful_block(body);
    if lines.is_empty() {
        return None;
    }

    let summary = lines.join(" ");
    let normalized = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }

    const LIMIT: usize = 180;
    let excerpt: String = normalized.chars().take(LIMIT).collect();
    if normalized.chars().count() > LIMIT {
        Some(format!("{excerpt}..."))
    } else {
        Some(excerpt)
    }
}

fn summarize_body_markdown(body: &str) -> Option<String> {
    let lines = first_meaningful_block(body);
    if lines.is_empty() {
        return None;
    }

    Some(lines.join("\n"))
}

fn build_edges(
    specs: &[SpecManifest],
    nodes: &[GraphNode],
) -> Vec<GraphEdge> {
    let known: BTreeSet<String> =
        nodes.iter().map(|node| node.id.clone()).collect();
    let mut edges = parent_edges(specs, &known);
    edges.extend(code_ref_edges(specs));
    edges
}

fn parent_edges(
    specs: &[SpecManifest],
    known: &BTreeSet<String>,
) -> Vec<GraphEdge> {
    let mut edges = Vec::new();
    for spec in specs {
        let Some(parent_id) = spec.parent() else {
            continue;
        };
        if known.contains(parent_id) {
            edges.push(GraphEdge {
                from: parent_id.to_string(),
                to: spec.id.to_string(),
                kind: "parent".to_string(),
            });
        }
    }
    edges
}

fn code_ref_edges(specs: &[SpecManifest]) -> Vec<GraphEdge> {
    let mut by_file: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let id_strings: Vec<String> =
        specs.iter().map(|spec| spec.id.to_string()).collect();

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
                        to: key.1,
                        kind: "code_ref".to_string(),
                    });
                }
            }
        }
    }

    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_body_skips_heading_and_truncates() {
        let body = concat!(
            "# Heading\n\n",
            "This is the first meaningful paragraph for the summary. ",
            "It should be preserved and collapsed into a single line even when the source uses multiple words.\n\n",
            "Second paragraph."
        );

        let summary = summarize_body(body).expect("summary should exist");

        assert!(summary.starts_with("This is the first meaningful paragraph"));
        assert!(!summary.contains("Heading"));
        assert!(!summary.contains("Second paragraph"));
    }

    #[test]
    fn summarize_body_markdown_preserves_inline_markdown_from_first_block() {
        let body = concat!(
            "# Heading\n\n",
            "This keeps *emphasis* and [links](https://example.test).\n",
            "Still the same block.\n\n",
            "Second paragraph."
        );

        let summary =
            summarize_body_markdown(body).expect("summary should exist");

        assert!(summary.contains("*emphasis*"));
        assert!(summary.contains("[links](https://example.test)"));
        assert!(!summary.contains("Heading"));
        assert!(!summary.contains("Second paragraph"));
    }

    #[test]
    fn count_children_tracks_immediate_children() {
        let parent = SpecManifest::new("context/root", "Root", "context");
        let parent_id = parent.id.to_string();

        let mut child_a = SpecManifest::new("context/a", "A", "context");
        child_a.set_parent(&parent_id);

        let mut child_b = SpecManifest::new("context/b", "B", "context");
        child_b.set_parent(&parent_id);

        let counts = count_children(&[parent, child_a, child_b]);

        assert_eq!(counts.get(&parent_id), Some(&2));
    }
}
