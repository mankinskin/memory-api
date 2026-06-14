// ticket-vscode-core
//
// Rust/WASM core for the ticket-vscode VS Code extension.
// Compiled to wasm32-unknown-unknown via wasm-pack.
//
// Design contract (frozen in spec ticket-vscode/rust-wasm-port):
// - No `vscode` or Node/browser APIs are imported here.
// - All host interaction goes through capability-object arguments passed in
//   from the JS/TS host shell at activation time.
// - #[wasm_bindgen] annotations are gated behind the "wasm" feature flag so
//   the crate builds and tests natively without wasm-pack.

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

// ── Version ───────────────────────────────────────────────────────────────────

/// Returns the core library version string.
/// Used by both hosts to confirm the WASM module loaded successfully.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ── Domain types ──────────────────────────────────────────────────────────────
//
// These mirror the shapes in `src/api.ts` and are the input feed from the JS
// host shell after fetching from the ticket-viewer HTTP API.

/// Minimal ticket summary — mirrors `TicketSummary` in `src/api.ts`.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Clone)]
pub struct TicketSummary {
    id: String,
    ticket_type: String,
    title: String,
    state: String,
}

#[cfg_attr(feature = "wasm", wasm_bindgen)]
impl TicketSummary {
    /// Construct a TicketSummary from the JS host.
    /// `title` and `state` are empty strings when the API returns null.
    #[cfg_attr(feature = "wasm", wasm_bindgen(constructor))]
    pub fn new(id: String, ticket_type: String, title: String, state: String) -> Self {
        Self { id, ticket_type, title, state }
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn id(&self) -> String { self.id.clone() }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn ticket_type(&self) -> String { self.ticket_type.clone() }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn title(&self) -> String { self.title.clone() }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn state(&self) -> String { self.state.clone() }
}

/// A directed edge between two tickets — mirrors `EdgeRecord` in `src/api.ts`.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Clone)]
pub struct EdgeRecord {
    from: String,
    to: String,
    kind: String,
}

#[cfg_attr(feature = "wasm", wasm_bindgen)]
impl EdgeRecord {
    #[cfg_attr(feature = "wasm", wasm_bindgen(constructor))]
    pub fn new(from: String, to: String, kind: String) -> Self {
        Self { from, to, kind }
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn from_id(&self) -> String { self.from.clone() }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn to_id(&self) -> String { self.to.clone() }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn kind(&self) -> String { self.kind.clone() }
}

// ── Host-kind detection ───────────────────────────────────────────────────────

/// Host kind reported by `HostDetectionCapability` in the JS shell.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKind {
    DesktopNode,
    RemoteWorkspace,
    BrowserWeb,
    Virtual,
}

/// Returns `true` when server-control features (startServer, binary spawn)
/// should be available for the given host kind.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn supports_server_control(host: HostKind) -> bool {
    matches!(host, HostKind::DesktopNode | HostKind::RemoteWorkspace)
}

/// Returns `true` when browser-bridge features (bridge* commands) should be
/// available for the given host kind.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn supports_browser_bridge(host: HostKind) -> bool {
    matches!(host, HostKind::DesktopNode)
}

/// Returns `true` when on-disk file browsing is available.
/// Desktop and remote workspace hosts have a real filesystem accessible via
/// `vscode.workspace.fs`; browser/virtual hosts treat this as best-effort.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn supports_file_browsing(host: HostKind) -> bool {
    matches!(host, HostKind::DesktopNode | HostKind::RemoteWorkspace)
}

// ── Filtering ─────────────────────────────────────────────────────────────────

/// Returns `true` when a ticket matches both state and query filters.
/// Pure function — no I/O, no VS Code APIs.
pub fn ticket_matches(ticket: &TicketSummary, state_filter: &str, query: &str) -> bool {
    if !state_filter.is_empty() && ticket.state != state_filter {
        return false;
    }
    if !query.is_empty() {
        let q = query.to_lowercase();
        if !ticket.title.to_lowercase().contains(&q)
            && !ticket.id.to_lowercase().contains(&q)
        {
            return false;
        }
    }
    true
}

fn filter_indices(tickets: &[TicketSummary], state_filter: &str, query: &str) -> Vec<usize> {
    tickets
        .iter()
        .enumerate()
        .filter(|(_, t)| ticket_matches(t, state_filter, query))
        .map(|(i, _)| i)
        .collect()
}

// ── Dependency maps ───────────────────────────────────────────────────────────

/// Pre-computed bidirectional lookup tables built from a flat edge list.
///
/// Equivalent to `_depsOf` / `_parentOf` / `_hasParent` in `TicketTreeProvider`.
pub struct DependencyMaps {
    /// ticket id → ids of tickets it depends_on (children in the tree)
    pub deps_of: std::collections::HashMap<String, Vec<String>>,
    /// ticket id → ids of its parents (reverse of deps_of)
    pub parent_of: std::collections::HashMap<String, Vec<String>>,
}

impl DependencyMaps {
    pub fn build(tickets: &[TicketSummary], edges: &[EdgeRecord]) -> Self {
        let known: std::collections::HashSet<&str> =
            tickets.iter().map(|t| t.id.as_str()).collect();

        let mut deps_of: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let mut parent_of: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for edge in edges {
            if edge.kind != "depends_on" { continue; }
            if !known.contains(edge.from.as_str()) || !known.contains(edge.to.as_str()) {
                continue;
            }
            deps_of.entry(edge.from.clone()).or_default().push(edge.to.clone());
            parent_of.entry(edge.to.clone()).or_default().push(edge.from.clone());
        }

        Self { deps_of, parent_of }
    }
}

// ── State grouping + root detection ──────────────────────────────────────────

/// A group of tickets that share the same state, as displayed in the sidebar.
///
/// Mirrors `StateGroupItem` / `buildStateGroups` from `ticketProvider.ts`.
#[derive(Debug, Clone)]
pub struct StateGroup {
    /// The state value (e.g. "ready", "in-implementation").
    pub state: String,
    /// Total tickets in this state bucket.
    pub total: usize,
    /// Ids of root tickets — those with no same-state parent.
    pub root_ids: Vec<String>,
}

/// Build state groups from tickets, edges, schema order, and active filters.
///
/// `state_order` is the schema-defined state list (empty = alphabetical).
/// `state_filter` and `query` are the current active filters.
pub fn build_state_groups(
    tickets: &[TicketSummary],
    edges: &[EdgeRecord],
    state_order: &[String],
    state_filter: &str,
    query: &str,
) -> Vec<StateGroup> {
    let maps = DependencyMaps::build(tickets, edges);
    let visible_indices = filter_indices(tickets, state_filter, query);
    let visible: Vec<&TicketSummary> = visible_indices.iter().map(|&i| &tickets[i]).collect();

    let mut grouped: std::collections::HashMap<&str, Vec<&TicketSummary>> =
        std::collections::HashMap::new();
    for t in &visible {
        grouped.entry(t.state.as_str()).or_default().push(t);
    }

    let make_group = |state: &str, bucket: &[&TicketSummary]| -> StateGroup {
        let state_ids: std::collections::HashSet<&str> =
            bucket.iter().map(|t| t.id.as_str()).collect();
        let root_ids: Vec<String> = bucket
            .iter()
            .filter(|t| {
                !maps.parent_of
                    .get(t.id.as_str())
                    .map_or(false, |ps| ps.iter().any(|p| state_ids.contains(p.as_str())))
            })
            .map(|t| t.id.clone())
            .collect();
        StateGroup { state: state.to_string(), total: bucket.len(), root_ids }
    };

    let mut result: Vec<StateGroup> = Vec::new();
    let mut remaining = grouped.clone();

    for s in state_order {
        if let Some(bucket) = remaining.remove(s.as_str()) {
            if !bucket.is_empty() {
                result.push(make_group(s.as_str(), &bucket));
            }
        }
    }
    let mut extra: Vec<(&str, Vec<&TicketSummary>)> = remaining.into_iter().collect();
    extra.sort_by_key(|(s, _)| *s);
    for (s, bucket) in extra {
        if !bucket.is_empty() {
            result.push(make_group(s, &bucket));
        }
    }

    result
}

// ── URL / command intent derivation ──────────────────────────────────────────

/// Returns the URL for opening a ticket in the ticket-viewer SPA.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn ticket_viewer_url(base_url: &str, workspace: &str, ticket_id: &str) -> String {
    format!("{base_url}/?workspace={workspace}&ticket={ticket_id}")
}

/// Returns the short display label for a ticket (first 8 chars of id if no title).
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn ticket_display_label(id: &str, title: &str) -> String {
    if title.is_empty() {
        format!("({})", &id[..id.len().min(8)])
    } else {
        title.to_string()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, title: &str, state: &str) -> TicketSummary {
        TicketSummary::new(id.into(), "tracker-improvement".into(), title.into(), state.into())
    }

    fn e(from: &str, to: &str) -> EdgeRecord {
        EdgeRecord::new(from.into(), to.into(), "depends_on".into())
    }

    #[test] fn version_is_non_empty() { assert!(!core_version().is_empty()); }

    // Host-kind gates
    #[test]
    fn host_kind_gates() {
        assert!(supports_server_control(HostKind::DesktopNode));
        assert!(supports_server_control(HostKind::RemoteWorkspace));
        assert!(!supports_server_control(HostKind::BrowserWeb));
        assert!(!supports_server_control(HostKind::Virtual));
        assert!(supports_browser_bridge(HostKind::DesktopNode));
        assert!(!supports_browser_bridge(HostKind::RemoteWorkspace));
        assert!(supports_file_browsing(HostKind::DesktopNode));
        assert!(supports_file_browsing(HostKind::RemoteWorkspace));
        assert!(!supports_file_browsing(HostKind::BrowserWeb));
        assert!(!supports_file_browsing(HostKind::Virtual));
    }

    // Filtering
    #[test] fn filter_by_state() {
        let ticket = t("a", "Add feature", "ready");
        assert!(ticket_matches(&ticket, "ready", ""));
        assert!(!ticket_matches(&ticket, "done", ""));
        assert!(ticket_matches(&ticket, "", ""));
    }
    #[test] fn filter_by_query() {
        let ticket = t("abc123", "Add feature", "ready");
        assert!(ticket_matches(&ticket, "", "feature"));
        assert!(ticket_matches(&ticket, "", "FEATURE"));
        assert!(!ticket_matches(&ticket, "", "missing"));
        assert!(ticket_matches(&ticket, "", "abc"));
    }
    #[test] fn filter_combined() {
        let ticket = t("a", "Add feature", "ready");
        assert!(ticket_matches(&ticket, "ready", "feature"));
        assert!(!ticket_matches(&ticket, "done", "feature"));
    }

    // Dependency maps
    #[test]
    fn dependency_maps_basic() {
        let tickets = vec![t("a", "Parent", "ready"), t("b", "Child", "ready")];
        let edges = vec![e("a", "b")];
        let maps = DependencyMaps::build(&tickets, &edges);
        assert_eq!(maps.deps_of["a"], vec!["b"]);
        assert_eq!(maps.parent_of["b"], vec!["a"]);
        assert!(!maps.deps_of.contains_key("b"));
        assert!(!maps.parent_of.contains_key("a"));
    }
    #[test]
    fn dependency_maps_skips_unknown() {
        let tickets = vec![t("a", "A", "ready")];
        let edges = vec![e("a", "unknown")];
        let maps = DependencyMaps::build(&tickets, &edges);
        assert!(!maps.deps_of.contains_key("a"));
    }
    #[test]
    fn dependency_maps_skips_non_depends_on() {
        let tickets = vec![t("a", "A", "ready"), t("b", "B", "ready")];
        let edges = vec![EdgeRecord::new("a".into(), "b".into(), "linked".into())];
        let maps = DependencyMaps::build(&tickets, &edges);
        assert!(!maps.deps_of.contains_key("a"));
    }

    // State grouping
    #[test]
    fn state_groups_roots() {
        let tickets = vec![t("a", "Parent", "ready"), t("b", "Child", "ready"), t("c", "Done", "done")];
        let edges = vec![e("a", "b")];
        let groups = build_state_groups(&tickets, &edges, &[], "", "");
        let done = groups.iter().find(|g| g.state == "done").unwrap();
        let ready = groups.iter().find(|g| g.state == "ready").unwrap();
        assert_eq!(done.total, 1);
        assert_eq!(ready.total, 2);
        assert_eq!(ready.root_ids, vec!["a"]);
    }
    #[test]
    fn state_groups_schema_order() {
        let tickets = vec![t("a", "A", "done"), t("b", "B", "ready")];
        let order: Vec<String> = vec!["ready".into(), "done".into()];
        let groups = build_state_groups(&tickets, &[], &order, "", "");
        assert_eq!(groups[0].state, "ready");
        assert_eq!(groups[1].state, "done");
    }
    #[test]
    fn state_groups_state_filter() {
        let tickets = vec![t("a", "A", "ready"), t("b", "B", "done")];
        let groups = build_state_groups(&tickets, &[], &[], "ready", "");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].state, "ready");
    }
    #[test]
    fn state_groups_query_filter() {
        let tickets = vec![t("a", "Alpha feature", "ready"), t("b", "Beta thing", "ready")];
        let groups = build_state_groups(&tickets, &[], &[], "", "alpha");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].root_ids, vec!["a"]);
    }

    // URL / intent
    #[test]
    fn url_format() {
        assert_eq!(
            ticket_viewer_url("http://localhost:3002", "default", "abc123"),
            "http://localhost:3002/?workspace=default&ticket=abc123"
        );
    }
    #[test] fn display_label_with_title() {
        assert_eq!(ticket_display_label("abc123", "My ticket"), "My ticket");
    }
    #[test] fn display_label_no_title() {
        assert_eq!(ticket_display_label("abcdef1234", ""), "(abcdef12)");
    }
    #[test] fn display_label_short_id() {
        assert_eq!(ticket_display_label("ab", ""), "(ab)");
    }
}
