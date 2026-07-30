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
    pub fn new(
        id: String,
        ticket_type: String,
        title: String,
        state: String,
    ) -> Self {
        Self {
            id,
            ticket_type,
            title,
            state,
        }
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn id(&self) -> String {
        self.id.clone()
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn ticket_type(&self) -> String {
        self.ticket_type.clone()
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn title(&self) -> String {
        self.title.clone()
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn state(&self) -> String {
        self.state.clone()
    }
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
    pub fn new(
        from: String,
        to: String,
        kind: String,
    ) -> Self {
        Self { from, to, kind }
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn from_id(&self) -> String {
        self.from.clone()
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn to_id(&self) -> String {
        self.to.clone()
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn kind(&self) -> String {
        self.kind.clone()
    }
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
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn ticket_matches(
    ticket: &TicketSummary,
    state_filter: &str,
    query: &str,
) -> bool {
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

fn filter_indices(
    tickets: &[TicketSummary],
    state_filter: &str,
    query: &str,
) -> Vec<usize> {
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
    pub fn build(
        tickets: &[TicketSummary],
        edges: &[EdgeRecord],
    ) -> Self {
        let known: std::collections::HashSet<&str> =
            tickets.iter().map(|t| t.id.as_str()).collect();

        let mut deps_of: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let mut parent_of: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for edge in edges {
            if edge.kind != "depends_on" {
                continue;
            }
            if !known.contains(edge.from.as_str())
                || !known.contains(edge.to.as_str())
            {
                continue;
            }
            deps_of
                .entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
            parent_of
                .entry(edge.to.clone())
                .or_default()
                .push(edge.from.clone());
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
    /// The state value (e.g. "planned", "in-implementation").
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
    let visible: Vec<&TicketSummary> =
        visible_indices.iter().map(|&i| &tickets[i]).collect();

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
                !maps.parent_of.get(t.id.as_str()).map_or(false, |ps| {
                    ps.iter().any(|p| state_ids.contains(p.as_str()))
                })
            })
            .map(|t| t.id.clone())
            .collect();
        StateGroup {
            state: state.to_string(),
            total: bucket.len(),
            root_ids,
        }
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
    let mut extra: Vec<(&str, Vec<&TicketSummary>)> =
        remaining.into_iter().collect();
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
pub fn ticket_viewer_url(
    base_url: &str,
    workspace: &str,
    ticket_id: &str,
) -> String {
    format!("{base_url}/?workspace={workspace}&ticket={ticket_id}")
}

/// Returns the short display label for a ticket (first 8 chars of id if no title).
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn ticket_display_label(
    id: &str,
    title: &str,
) -> String {
    if title.is_empty() {
        format!("({})", &id[..id.len().min(8)])
    } else {
        title.to_string()
    }
}

// ── WASM-specific wrappers ─────────────────────────────────────────────────────
//
// These types and functions expose the domain logic to JavaScript.
// Bulk inputs (ticket lists, edge lists) are passed as parallel `js_sys::Array`
// instances of strings rather than arrays of Rust structs, because wasm-bindgen
// does not support extracting custom struct types from JsValue arrays.
// Individual TicketSummary objects CAN be passed directly to ticket_matches
// (wasm-bindgen handles &T parameters via RefFromWasmAbi).

#[cfg(feature = "wasm")]
mod wasm_api {
    use super::*;
    use js_sys::Array;
    #[allow(unused_imports)]
    use wasm_bindgen::prelude::*;

    // ── helpers ─────────────────────────────────────────────────────────────

    /// Extract a Vec<String> from a js_sys::Array of JS string values.
    fn str_vec(arr: &Array) -> Vec<String> {
        arr.iter().filter_map(|v| v.as_string()).collect()
    }

    /// Build TicketSummary list from parallel id / title / state arrays.
    fn tickets_from_arrays(
        ids: &Array,
        titles: &Array,
        states: &Array,
    ) -> Vec<TicketSummary> {
        let n = ids.length() as usize;
        (0..n)
            .map(|i| {
                let i = i as u32;
                TicketSummary::new(
                    ids.get(i).as_string().unwrap_or_default(),
                    String::new(), // ticket_type not needed for domain logic
                    titles.get(i).as_string().unwrap_or_default(),
                    states.get(i).as_string().unwrap_or_default(),
                )
            })
            .collect()
    }

    /// Build EdgeRecord list from parallel from / to / kind arrays.
    fn edges_from_arrays(
        froms: &Array,
        tos: &Array,
        kinds: &Array,
    ) -> Vec<EdgeRecord> {
        let n = froms.length() as usize;
        (0..n)
            .map(|i| {
                let i = i as u32;
                EdgeRecord::new(
                    froms.get(i).as_string().unwrap_or_default(),
                    tos.get(i).as_string().unwrap_or_default(),
                    kinds.get(i).as_string().unwrap_or_default(),
                )
            })
            .collect()
    }

    // ── WasmDependencyMaps ───────────────────────────────────────────────────

    /// WASM-exposed DependencyMaps wrapper.
    ///
    /// Inputs are parallel `Array<string>` values rather than typed struct arrays.
    #[wasm_bindgen(js_name = "WasmDependencyMaps")]
    pub struct WasmDependencyMaps {
        inner: super::DependencyMaps,
    }

    #[wasm_bindgen(js_class = "WasmDependencyMaps")]
    impl WasmDependencyMaps {
        /// Build the dependency maps.
        ///
        /// - `ticket_ids`: Array of ticket ID strings.
        /// - `edge_froms`, `edge_tos`, `edge_kinds`: parallel edge arrays.
        #[wasm_bindgen(js_name = "build")]
        pub fn build_js(
            ticket_ids: &Array,
            edge_froms: &Array,
            edge_tos: &Array,
            edge_kinds: &Array,
        ) -> WasmDependencyMaps {
            // DependencyMaps only needs IDs; create minimal TicketSummary stubs.
            let tickets: Vec<TicketSummary> = str_vec(ticket_ids)
                .into_iter()
                .map(|id| {
                    TicketSummary::new(
                        id,
                        String::new(),
                        String::new(),
                        String::new(),
                    )
                })
                .collect();
            let edges = edges_from_arrays(edge_froms, edge_tos, edge_kinds);
            WasmDependencyMaps {
                inner: super::DependencyMaps::build(&tickets, &edges),
            }
        }

        /// Returns an Array of dependency IDs for the given ticket ID.
        #[wasm_bindgen(js_name = "depsOf")]
        pub fn deps_of_js(
            &self,
            id: &str,
        ) -> Array {
            self.inner.deps_of.get(id).map_or_else(Array::new, |deps| {
                deps.iter().map(|d| JsValue::from_str(d)).collect::<Array>()
            })
        }

        /// Returns an Array of parent ticket IDs for the given ticket ID.
        #[wasm_bindgen(js_name = "parentOf")]
        pub fn parent_of_js(
            &self,
            id: &str,
        ) -> Array {
            self.inner.parent_of.get(id).map_or_else(Array::new, |ps| {
                ps.iter().map(|p| JsValue::from_str(p)).collect::<Array>()
            })
        }

        /// Returns true when the ticket has at least one parent.
        #[wasm_bindgen(js_name = "hasParent")]
        pub fn has_parent_js(
            &self,
            id: &str,
        ) -> bool {
            self.inner.parent_of.contains_key(id)
        }
    }

    // ── WasmStateGroup ───────────────────────────────────────────────────────

    /// WASM-friendly state group returned by `buildStateGroups`.
    #[wasm_bindgen(js_name = "WasmStateGroup")]
    pub struct WasmStateGroup {
        state: String,
        pub total: usize,
        root_ids: Vec<String>,
    }

    #[wasm_bindgen(js_class = "WasmStateGroup")]
    impl WasmStateGroup {
        #[wasm_bindgen(getter)]
        pub fn state(&self) -> String {
            self.state.clone()
        }

        /// Returns a JS Array of root ticket IDs for this state group.
        #[wasm_bindgen(js_name = "rootIds")]
        pub fn root_ids_js(&self) -> Array {
            self.root_ids
                .iter()
                .map(|id| JsValue::from_str(id))
                .collect::<Array>()
        }
    }

    // ── buildStateGroups ─────────────────────────────────────────────────────

    /// Compute state groups from parallel string arrays.
    ///
    /// - `ticket_ids`, `ticket_titles`, `ticket_states`: parallel ticket arrays.
    /// - `edge_froms`, `edge_tos`, `edge_kinds`: parallel edge arrays.
    /// - `state_order`: schema-defined state ordering (empty = alphabetical).
    /// - `state_filter`: server-side state filter already applied (pass `""` if pre-filtered).
    /// - `query`: client-side search string (case-insensitive substring match on title + id).
    ///
    /// Returns a JS `Array` of `WasmStateGroup` objects.
    #[wasm_bindgen(js_name = "buildStateGroups")]
    pub fn build_state_groups_wasm(
        ticket_ids: &Array,
        ticket_titles: &Array,
        ticket_states: &Array,
        edge_froms: &Array,
        edge_tos: &Array,
        edge_kinds: &Array,
        state_order: &Array,
        state_filter: &str,
        query: &str,
    ) -> Array {
        let tickets =
            tickets_from_arrays(ticket_ids, ticket_titles, ticket_states);
        let edges = edges_from_arrays(edge_froms, edge_tos, edge_kinds);
        let state_order_vec = str_vec(state_order);

        super::build_state_groups(
            &tickets,
            &edges,
            &state_order_vec,
            state_filter,
            query,
        )
        .into_iter()
        .map(|g| {
            let wsg = WasmStateGroup {
                state: g.state,
                total: g.total,
                root_ids: g.root_ids,
            };
            JsValue::from(wsg)
        })
        .collect::<Array>()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn t(
        id: &str,
        title: &str,
        state: &str,
    ) -> TicketSummary {
        TicketSummary::new(
            id.into(),
            "tracker-improvement".into(),
            title.into(),
            state.into(),
        )
    }

    fn e(
        from: &str,
        to: &str,
    ) -> EdgeRecord {
        EdgeRecord::new(from.into(), to.into(), "depends_on".into())
    }

    #[test]
    fn version_is_non_empty() {
        assert!(!core_version().is_empty());
    }

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
    #[test]
    fn filter_by_state() {
        let ticket = t("a", "Add feature", "planned");
        assert!(ticket_matches(&ticket, "planned", ""));
        assert!(!ticket_matches(&ticket, "done", ""));
        assert!(ticket_matches(&ticket, "", ""));
    }
    #[test]
    fn filter_by_query() {
        let ticket = t("abc123", "Add feature", "planned");
        assert!(ticket_matches(&ticket, "", "feature"));
        assert!(ticket_matches(&ticket, "", "FEATURE"));
        assert!(!ticket_matches(&ticket, "", "missing"));
        assert!(ticket_matches(&ticket, "", "abc"));
    }
    #[test]
    fn filter_combined() {
        let ticket = t("a", "Add feature", "planned");
        assert!(ticket_matches(&ticket, "planned", "feature"));
        assert!(!ticket_matches(&ticket, "done", "feature"));
    }

    // Dependency maps
    #[test]
    fn dependency_maps_basic() {
        let tickets = vec![t("a", "Parent", "planned"), t("b", "Child", "planned")];
        let edges = vec![e("a", "b")];
        let maps = DependencyMaps::build(&tickets, &edges);
        assert_eq!(maps.deps_of["a"], vec!["b"]);
        assert_eq!(maps.parent_of["b"], vec!["a"]);
        assert!(!maps.deps_of.contains_key("b"));
        assert!(!maps.parent_of.contains_key("a"));
    }
    #[test]
    fn dependency_maps_skips_unknown() {
        let tickets = vec![t("a", "A", "planned")];
        let edges = vec![e("a", "unknown")];
        let maps = DependencyMaps::build(&tickets, &edges);
        assert!(!maps.deps_of.contains_key("a"));
    }
    #[test]
    fn dependency_maps_skips_non_depends_on() {
        let tickets = vec![t("a", "A", "planned"), t("b", "B", "planned")];
        let edges =
            vec![EdgeRecord::new("a".into(), "b".into(), "linked".into())];
        let maps = DependencyMaps::build(&tickets, &edges);
        assert!(!maps.deps_of.contains_key("a"));
    }

    // State grouping
    #[test]
    fn state_groups_roots() {
        let tickets = vec![
            t("a", "Parent", "planned"),
            t("b", "Child", "planned"),
            t("c", "Done", "done"),
        ];
        let edges = vec![e("a", "b")];
        let groups = build_state_groups(&tickets, &edges, &[], "", "");
        let done = groups.iter().find(|g| g.state == "done").unwrap();
        let ready = groups.iter().find(|g| g.state == "planned").unwrap();
        assert_eq!(done.total, 1);
        assert_eq!(ready.total, 2);
        assert_eq!(ready.root_ids, vec!["a"]);
    }
    #[test]
    fn state_groups_schema_order() {
        let tickets = vec![t("a", "A", "done"), t("b", "B", "planned")];
        let order: Vec<String> = vec!["planned".into(), "done".into()];
        let groups = build_state_groups(&tickets, &[], &order, "", "");
        assert_eq!(groups[0].state, "planned");
        assert_eq!(groups[1].state, "done");
    }
    #[test]
    fn state_groups_state_filter() {
        let tickets = vec![t("a", "A", "planned"), t("b", "B", "done")];
        let groups = build_state_groups(&tickets, &[], &[], "planned", "");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].state, "planned");
    }
    #[test]
    fn state_groups_query_filter() {
        let tickets = vec![
            t("a", "Alpha feature", "planned"),
            t("b", "Beta thing", "planned"),
        ];
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
    #[test]
    fn display_label_with_title() {
        assert_eq!(ticket_display_label("abc123", "My ticket"), "My ticket");
    }
    #[test]
    fn display_label_no_title() {
        assert_eq!(ticket_display_label("abcdef1234", ""), "(abcdef12)");
    }
    #[test]
    fn display_label_short_id() {
        assert_eq!(ticket_display_label("ab", ""), "(ab)");
    }
}
