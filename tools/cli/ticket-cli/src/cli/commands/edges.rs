use std::{
    collections::{
        HashMap,
        HashSet,
        VecDeque,
    },
    fmt::Write,
};

use chrono::Utc;
use serde_json::{
    Value,
    json,
};
use uuid::Uuid;

use ticket_api::{
    model::edge::EdgeRecord,
    storage::TicketStore,
};

use crate::cli::{
    CliRunError,
    LinkArgs,
    LinksArgs,
    SubgraphArgs,
    TopgraphArgs,
    UnlinkArgs,
};

pub(crate) fn cmd_link(
    args: LinkArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let from = resolve_uuid_prefix(&args.from, store)?;
    let to = resolve_uuid_prefix(&args.to, store)?;
    let from_title = store
        .get(&from)
        .ok()
        .and_then(|m| {
            m.extra
                .get("title")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_else(|| from.to_string());
    let to_title = store
        .get(&to)
        .ok()
        .and_then(|m| {
            m.extra
                .get("title")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_else(|| to.to_string());
    let edge = EdgeRecord {
        from,
        to,
        kind: args.kind.clone(),
        created_at: Utc::now(),
    };
    store.add_edge(edge)?;
    Ok(json!({
        "command": "link",
        "status": "ok",
        "from": from,
        "from_title": from_title,
        "to": to,
        "to_title": to_title,
        "kind": args.kind,
        "reason": args.reason,
    }))
}

pub(crate) fn cmd_unlink(
    args: UnlinkArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let from = resolve_uuid_prefix(&args.from, store)?;
    let to = resolve_uuid_prefix(&args.to, store)?;
    let from_title = store
        .get(&from)
        .ok()
        .and_then(|m| {
            m.extra
                .get("title")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_else(|| from.to_string());
    let to_title = store
        .get(&to)
        .ok()
        .and_then(|m| {
            m.extra
                .get("title")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_else(|| to.to_string());
    let edge = EdgeRecord {
        from,
        to,
        kind: args.kind.clone(),
        created_at: Utc::now(),
    };
    store.remove_edge(edge)?;
    Ok(json!({
        "command": "unlink",
        "status": "ok",
        "from": from,
        "from_title": from_title,
        "to": to,
        "to_title": to_title,
        "kind": args.kind,
        "reason": args.reason,
    }))
}

pub(crate) fn cmd_links(
    args: LinksArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let raw_edges = if args.all {
        store.list_all_edges()?
    } else {
        let id_str = args
            .id
            .as_ref()
            .expect("clap ensures id is present when --all is not set");
        let id = resolve_uuid_prefix(id_str, store)?;
        store.edges_from(&id)?
    };

    let items: Vec<Value> = raw_edges
        .iter()
        .filter(|e| match &args.kind {
            Some(k) => e.kind == *k,
            None => true,
        })
        .map(|e| json!({ "from": e.from, "to": e.to, "kind": e.kind }))
        .collect();
    Ok(json!({
        "command": "links",
        "status": "ok",
        "id": args.id,
        "all": args.all,
        "count": items.len(),
        "edges": items,
    }))
}

// ── subgraph / topgraph ────────────────────────────────────────────────────────

pub(crate) fn cmd_subgraph(
    args: SubgraphArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    graph_traversal(
        "subgraph",
        &args.root,
        args.depth,
        &args.direction,
        &args.edge_kind,
        false,
        store,
    )
}

pub(crate) fn cmd_topgraph(
    args: TopgraphArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    graph_traversal(
        "topgraph",
        &args.root,
        args.depth,
        &args.direction,
        &args.edge_kind,
        true,
        store,
    )
}

type TraversalNode = (Uuid, Option<String>, Option<String>, usize);

struct TraversalData {
    node_list: Vec<TraversalNode>,
    unique_edges: Vec<EdgeRecord>,
}

fn graph_traversal(
    command_name: &str,
    root_str: &str,
    depth: usize,
    direction: &str,
    edge_kind_filter: &str,
    reverse_tree: bool,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let root = resolve_uuid_prefix(root_str, store)?;
    let traversal = collect_graph_data(
        root,
        depth.min(8),
        direction,
        edge_kind_filter,
        store,
    )?;
    let json_nodes = graph_json_nodes(&traversal.node_list);
    let json_edges = graph_json_edges(&traversal.unique_edges);
    let edge_refs: Vec<&EdgeRecord> = traversal.unique_edges.iter().collect();
    let tree =
        render_ascii_tree(root, &traversal.node_list, &edge_refs, reverse_tree);

    Ok(json!({
        "command": command_name,
        "status": "ok",
        "tree": tree,
        "nodes": json_nodes,
        "edges": json_edges,
        "truncated": false,
        "stats": {
            "nodes_returned": json_nodes.len(),
            "edges_returned": json_edges.len(),
        },
    }))
}

fn collect_graph_data(
    root: Uuid,
    depth_limit: usize,
    direction: &str,
    edge_kind_filter: &str,
    store: &TicketStore,
) -> Result<TraversalData, CliRunError> {
    let all_edges = store.list_all_edges()?;
    let mut visited = HashSet::new();
    let mut node_list = Vec::new();
    let mut collected_edges = Vec::new();
    let mut queue = VecDeque::from([(root, 0)]);

    while let Some((current_id, depth)) = queue.pop_front() {
        if !visited.insert(current_id) {
            continue;
        }

        let (title, state) = indexed_ticket_state(store, current_id)?;
        node_list.push((current_id, title, state, depth));

        if depth >= depth_limit {
            continue;
        }

        for edge in &all_edges {
            let Some(neighbor) = matching_neighbor(
                edge,
                current_id,
                direction,
                edge_kind_filter,
            ) else {
                continue;
            };
            collected_edges.push(edge.clone());
            if !visited.contains(&neighbor) {
                queue.push_back((neighbor, depth + 1));
            }
        }
    }

    Ok(TraversalData {
        node_list,
        unique_edges: dedupe_edges(collected_edges),
    })
}

fn indexed_ticket_state(
    store: &TicketStore,
    ticket_id: Uuid,
) -> Result<(Option<String>, Option<String>), CliRunError> {
    Ok(match store.get_indexed(&ticket_id)? {
        Some(ticket) if !ticket.deleted => (ticket.title, ticket.state),
        _ => (None, None),
    })
}

fn matching_neighbor(
    edge: &EdgeRecord,
    current_id: Uuid,
    direction: &str,
    edge_kind_filter: &str,
) -> Option<Uuid> {
    if edge_kind_filter != "all" && edge.kind != edge_kind_filter {
        return None;
    }

    let (neighbor, is_outbound) = edge_neighbor(edge, current_id)?;
    if direction_allows(direction, is_outbound) {
        Some(neighbor)
    } else {
        None
    }
}

fn edge_neighbor(
    edge: &EdgeRecord,
    current_id: Uuid,
) -> Option<(Uuid, bool)> {
    if edge.from == current_id {
        Some((edge.to, true))
    } else if edge.to == current_id {
        Some((edge.from, false))
    } else {
        None
    }
}

fn direction_allows(
    direction: &str,
    is_outbound: bool,
) -> bool {
    match direction {
        "out" => is_outbound,
        "in" => !is_outbound,
        _ => true,
    }
}

fn dedupe_edges(collected_edges: Vec<EdgeRecord>) -> Vec<EdgeRecord> {
    let mut seen_edges = HashSet::new();
    collected_edges
        .into_iter()
        .filter(|edge| {
            seen_edges.insert((edge.from, edge.to, edge.kind.clone()))
        })
        .collect()
}

fn graph_json_nodes(node_list: &[TraversalNode]) -> Vec<Value> {
    node_list
        .iter()
        .map(|(id, title, state, depth)| {
            json!({
                "id": id.to_string(),
                "title": title,
                "state": state,
                "depth": depth,
            })
        })
        .collect()
}

fn graph_json_edges(unique_edges: &[EdgeRecord]) -> Vec<Value> {
    unique_edges
        .iter()
        .map(|edge| {
            json!({
                "from": edge.from.to_string(),
                "to": edge.to.to_string(),
                "kind": edge.kind,
            })
        })
        .collect()
}

use super::resolve_uuid_prefix;

/// Map an edge kind to its reverse-direction display label.
/// `depends_on` becomes `blocks`; unknown kinds get a `~` prefix.
fn reverse_edge_label(kind: &str) -> String {
    match kind {
        "depends_on" => "blocks".to_string(),
        other => format!("~{other}"),
    }
}

/// Render an ASCII dependency tree from BFS-collected nodes and edges.
/// When `reverse_tree` is true, edges are displayed from `to → from` (dependents).
fn render_ascii_tree(
    root: Uuid,
    nodes: &[(Uuid, Option<String>, Option<String>, usize)],
    edges: &[&EdgeRecord],
    reverse_tree: bool,
) -> String {
    // Build lookup: id -> (title, state)
    let node_info: HashMap<Uuid, (&Option<String>, &Option<String>)> = nodes
        .iter()
        .map(|(id, title, state, _)| (*id, (title, state)))
        .collect();

    // Build adjacency: tree-parent -> [(kind, tree-child)]
    // For subgraph (reverse_tree=false): from is parent, to is child
    // For topgraph (reverse_tree=true): to is parent, from is child
    let mut children: HashMap<Uuid, Vec<(&str, Uuid)>> = HashMap::new();
    for edge in edges {
        if reverse_tree {
            children
                .entry(edge.to)
                .or_default()
                .push((&edge.kind, edge.from));
        } else {
            children
                .entry(edge.from)
                .or_default()
                .push((&edge.kind, edge.to));
        }
    }

    let mut out = String::new();
    let short_id = &root.to_string()[..8];
    let (title, state) = node_info
        .get(&root)
        .map(|(t, s)| {
            (t.as_deref().unwrap_or("?"), s.as_deref().unwrap_or("?"))
        })
        .unwrap_or(("?", "?"));
    let _ = writeln!(out, "{title} ({short_id}) [{state}]");

    // Track visited to handle diamond dependencies
    let mut visited = HashSet::new();
    visited.insert(root);

    render_children(
        &mut out,
        &mut visited,
        root,
        &children,
        &node_info,
        "",
        reverse_tree,
    );
    out
}

fn render_children(
    out: &mut String,
    visited: &mut HashSet<Uuid>,
    parent: Uuid,
    children: &HashMap<Uuid, Vec<(&str, Uuid)>>,
    node_info: &HashMap<Uuid, (&Option<String>, &Option<String>)>,
    prefix: &str,
    reverse_tree: bool,
) {
    let Some(kids) = children.get(&parent) else {
        return;
    };

    for (i, (kind, child_id)) in kids.iter().enumerate() {
        let is_last = i == kids.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };

        let short_id = &child_id.to_string()[..8];
        let (title, state) = node_info
            .get(child_id)
            .map(|(t, s)| {
                (t.as_deref().unwrap_or("?"), s.as_deref().unwrap_or("?"))
            })
            .unwrap_or(("?", "?"));

        // In a topgraph the child actually depends on the parent, so flip the arrow.
        let edge_label = if reverse_tree {
            format!("{} →", reverse_edge_label(kind))
        } else {
            format!("{kind} →")
        };

        let already_visited = !visited.insert(*child_id);
        if already_visited {
            let _ = writeln!(
                out,
                "{prefix}{connector}{edge_label} {title} ({short_id}) [{state}] (→ see above)"
            );
        } else {
            let _ = writeln!(
                out,
                "{prefix}{connector}{edge_label} {title} ({short_id}) [{state}]"
            );
            let next_prefix = format!("{prefix}{child_prefix}");
            render_children(
                out,
                visited,
                *child_id,
                children,
                node_info,
                &next_prefix,
                reverse_tree,
            );
        }
    }
}
