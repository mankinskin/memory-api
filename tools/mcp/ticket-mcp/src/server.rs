use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars::{self, JsonSchema},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use uuid::Uuid;

use ticket_api::storage::store::TicketStore;
use ticket_api::storage::ticket_fs::TicketFs;

// ── Output types ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct TicketSummary {
    id: String,
    #[serde(rename = "type")]
    type_id: String,
    title: Option<String>,
    state: Option<String>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct TicketDetail {
    id: String,
    created_at: chrono::DateTime<chrono::Utc>,
    fields: BTreeMap<String, Value>,
}

#[derive(Serialize)]
struct EdgeItem {
    from: String,
    to: String,
    kind: String,
}

#[derive(Serialize)]
struct NodeItem {
    id: String,
    title: Option<String>,
    state: Option<String>,
    depth: usize,
}

#[derive(Serialize)]
struct SubgraphResponse {
    workspace: String,
    nodes: Vec<NodeItem>,
    edges: Vec<EdgeItem>,
    truncated: bool,
    stats: SubgraphStats,
}

#[derive(Serialize)]
struct SubgraphStats {
    nodes_returned: usize,
    edges_returned: usize,
    max_depth_reached: usize,
}

// ── Input types ──────────────────────────────────────────────────────────────

// ── Input types ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTicketsInput {
    pub workspace: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TicketRefInput {
    pub workspace: String,
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListEdgesInput {
    pub workspace: String,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SubgraphInput {
    pub workspace: String,
    pub root: String,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub edge_kind: Option<String>,
    #[serde(default)]
    pub depth: Option<usize>,
    #[serde(default)]
    pub limit_nodes: Option<usize>,
    #[serde(default)]
    pub limit_edges: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopgraphInput {
    pub workspace: String,
    pub root: String,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub edge_kind: Option<String>,
    #[serde(default)]
    pub depth: Option<usize>,
    #[serde(default)]
    pub limit_nodes: Option<usize>,
    #[serde(default)]
    pub limit_edges: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HealthCheckInput {
    pub workspace: String,
    /// Root ticket for BFS subgraph scope (optional if `all` or `ids` is set).
    #[serde(default)]
    pub root: Option<String>,
    /// Check all tickets in the workspace.
    #[serde(default)]
    pub all: bool,
    /// Explicit list of ticket IDs/prefixes to check (overrides root/all).
    #[serde(default)]
    pub ids: Vec<String>,
    /// BFS depth limit (default: 6, max: 8).
    #[serde(default)]
    pub depth: Option<usize>,
    /// BFS direction: out, in, or both (default: out).
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowName {
    List,
    TriageOpenTickets,
    FetchTicketContext,
    InspectDependencies,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateTicketInput {
    pub workspace: String,
    pub id: String,
    /// Optional state to transition to.
    #[serde(default)]
    pub to_state: Option<String>,
    /// Field patches as key=value pairs (e.g. ["priority=high", "owner=alice"]).
    #[serde(default)]
    pub fields: Vec<String>,
    /// If true, revert the ticket to its previous history revision (undo last change).
    #[serde(default)]
    pub undo: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CloseTicketInput {
    pub workspace: String,
    pub id: String,
    /// Target state to fast-forward to (default: "done").
    #[serde(default = "default_close_state")]
    pub to_state: String,
}

fn default_close_state() -> String {
    "done".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CancelTicketInput {
    pub workspace: String,
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowInput {
    #[serde(default = "default_workflow_name")]
    pub name: WorkflowName,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NextTicketsInput {
    pub workspace: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub filter: Option<String>,
}

fn default_workflow_name() -> WorkflowName {
    WorkflowName::List
}

// ── Server ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TicketServer {
    index_root: PathBuf,
    tool_router: ToolRouter<Self>,
    /// Serializes all `TicketStore::open` / drop cycles so that concurrent
    /// MCP tool calls never race on the redb file lock, while still releasing
    /// the lock between calls so the CLI and other processes can access the
    /// database.
    store_lock: Arc<Mutex<()>>,
}

impl TicketServer {
    pub fn new(index_root: PathBuf) -> Self {
        Self {
            index_root,
            tool_router: Self::tool_router(),
            store_lock: Arc::new(Mutex::new(())),
        }
    }

    fn json_result<T: Serialize>(value: &T) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(value)
            .map_err(|e| McpError::internal_error(format!("serialization: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn store_err(e: ticket_api::error::StorageError) -> McpError {
        McpError::internal_error(format!("store error: {e}"), None)
    }

    /// Resolve a UUID string — accepts full UUIDs directly and hex prefixes
    /// (>= 8 chars) with a store lookup.
    ///
    /// When an existing `&TicketStore` is available (inside `with_store`), pass
    /// it to avoid a redundant open.  Otherwise pass `None` and this method
    /// opens its own store (through the lock).
    fn resolve_uuid_with(
        store: &TicketStore,
        s: &str,
    ) -> Result<Uuid, McpError> {
        // Try full UUID parse first.
        if let Ok(id) = s.parse::<Uuid>() {
            return Ok(id);
        }

        // Allow hex prefix of at least 8 characters.
        let trimmed = s.trim();
        if trimmed.len() >= 8 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            let tickets = store.list(None, None, None).map_err(Self::store_err)?;
            let prefix_lower = trimmed.to_ascii_lowercase();
            let matches: Vec<Uuid> = tickets
                .iter()
                .filter(|t| {
                    // simple-format UUID (no hyphens) for prefix comparison
                    t.id.simple().to_string().starts_with(&prefix_lower)
                })
                .map(|t| t.id)
                .collect();

            return match matches.len() {
                1 => Ok(matches[0]),
                0 => Err(McpError::invalid_params(
                    format!("no ticket found matching prefix '{trimmed}'"),
                    None,
                )),
                n => Err(McpError::invalid_params(
                    format!("ambiguous prefix '{trimmed}': matches {n} tickets"),
                    None,
                )),
            };
        }

        Err(McpError::invalid_params(
            format!("invalid UUID '{s}': expected full UUID or hex prefix (>= 8 chars)"),
            None,
        ))
    }

    /// Open the store under the serialization lock, run `f`, then drop both
    /// store and lock before returning.  This guarantees the redb file lock
    /// is released before the MCP response is sent.
    ///
    /// The closure returns `StorageError`; it is mapped to `McpError`
    /// automatically.
    async fn with_store<T>(
        &self,
        f: impl FnOnce(&TicketStore) -> Result<T, ticket_api::error::StorageError>,
    ) -> Result<T, McpError> {
        let _guard = self.store_lock.lock().await;
        let store = TicketStore::open(&self.index_root).map_err(Self::store_err)?;
        let result = f(&store).map_err(Self::store_err);
        drop(store);
        result
    }

    /// Like `with_store`, but the closure may produce `McpError` directly
    /// (e.g. when calling `resolve_uuid_with` inside the closure).
    async fn with_store_ext<T>(
        &self,
        f: impl FnOnce(&TicketStore) -> Result<T, McpError>,
    ) -> Result<T, McpError> {
        let _guard = self.store_lock.lock().await;
        let store = TicketStore::open(&self.index_root).map_err(Self::store_err)?;
        let result = f(&store);
        drop(store);
        result
    }

    async fn bfs_graph(
        &self,
        workspace: String,
        root_str: &str,
        direction: &str,
        edge_kind: Option<&str>,
        depth: Option<usize>,
        limit_nodes: Option<usize>,
        limit_edges: Option<usize>,
    ) -> Result<CallToolResult, McpError> {
        let root_str = root_str.to_owned();
        let direction = direction.to_owned();
        let edge_kind = edge_kind.map(|s| s.to_owned());
        self.with_store_ext(move |store| {
        let root = Self::resolve_uuid_with(store, &root_str)?;
        let depth_limit = depth.unwrap_or(2).min(8);
        let node_limit = limit_nodes.unwrap_or(500);
        let edge_limit = limit_edges.unwrap_or(2000);
        let edge_kind_str = edge_kind.as_deref().unwrap_or("all");

        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut nodes: Vec<NodeItem> = Vec::new();
        let mut edges: Vec<EdgeItem> = Vec::new();
        let mut truncated = false;
        let mut max_depth_reached = 0;

        let mut queue: VecDeque<(Uuid, usize)> = VecDeque::new();
        queue.push_back((root, 0));

        while let Some((current_id, current_depth)) = queue.pop_front() {
            if visited.contains(&current_id) {
                continue;
            }
            if nodes.len() >= node_limit {
                truncated = true;
                break;
            }

            visited.insert(current_id);
            max_depth_reached = max_depth_reached.max(current_depth);

            let node = match store.get_indexed(&current_id) {
                Ok(Some(t)) => NodeItem {
                    id: current_id.to_string(),
                    title: t.title,
                    state: t.state,
                    depth: current_depth,
                },
                Ok(None) => NodeItem {
                    id: current_id.to_string(),
                    title: None,
                    state: None,
                    depth: current_depth,
                },
                Err(e) => return Err(Self::store_err(e)),
            };
            nodes.push(node);

            if current_depth >= depth_limit {
                continue;
            }

            let all_edges = store.list_all_edges().map_err(Self::store_err)?;

            for edge in &all_edges {
                let kind_ok = edge_kind_str == "all" || edge.kind == edge_kind_str;
                if !kind_ok {
                    continue;
                }

                let (neighbor, is_outbound) = if edge.from == current_id {
                    (edge.to, true)
                } else if edge.to == current_id {
                    (edge.from, false)
                } else {
                    continue;
                };

                let dir_ok = match direction.as_str() {
                    "out" => is_outbound,
                    "in" => !is_outbound,
                    _ => true,
                };
                if !dir_ok {
                    continue;
                }

                if edges.len() < edge_limit {
                    edges.push(EdgeItem {
                        from: edge.from.to_string(),
                        to: edge.to.to_string(),
                        kind: edge.kind.clone(),
                    });
                }

                if !visited.contains(&neighbor) {
                    queue.push_back((neighbor, current_depth + 1));
                }
            }
        }

        edges.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.kind == b.kind);

        let stats = SubgraphStats {
            nodes_returned: nodes.len(),
            edges_returned: edges.len(),
            max_depth_reached,
        };
        Self::json_result(&SubgraphResponse {
            workspace,
            nodes,
            edges,
            truncated,
            stats,
        })
        }).await
    }

    async fn run_health_checks(
        &self,
        workspace: &str,
        root: Option<&str>,
        all: bool,
        ids: &[String],
        depth: Option<usize>,
        direction: Option<&str>,
    ) -> Result<CallToolResult, McpError> {
        let workspace = workspace.to_owned();
        let root = root.map(|s| s.to_owned());
        let ids = ids.to_owned();
        let direction = direction.map(|s| s.to_owned());
        self.with_store_ext(move |store| {
        let all_edges = store.list_all_edges().map_err(Self::store_err)?;

        // Collect tickets in scope.
        let tickets = if !ids.is_empty() {
            let mut result = Vec::new();
            for id_str in &ids {
                let id = Self::resolve_uuid_with(store, id_str)?;
                if let Some(t) = store.get_indexed(&id).map_err(Self::store_err)? {
                    if !t.deleted {
                        result.push(t);
                    }
                }
            }
            result
        } else if all {
            store.list(None, None, None).map_err(Self::store_err)?
        } else {
            let root_str = root.as_deref().ok_or_else(|| {
                McpError::invalid_params("one of 'root', 'all', or 'ids' is required", None)
            })?;
            let root_id = Self::resolve_uuid_with(store, root_str)?;
            let depth_limit = depth.unwrap_or(6).min(8);
            let direction_str = direction.as_deref().unwrap_or("out");

            let mut visited: HashSet<Uuid> = HashSet::new();
            let mut collected_ids: Vec<Uuid> = Vec::new();
            let mut queue: VecDeque<(Uuid, usize)> = VecDeque::new();
            queue.push_back((root_id, 0));

            while let Some((current_id, d)) = queue.pop_front() {
                if !visited.insert(current_id) {
                    continue;
                }
                collected_ids.push(current_id);
                if d >= depth_limit {
                    continue;
                }
                for edge in &all_edges {
                    let kind_ok = edge.kind == "depends_on" || edge.kind == "linked";
                    if !kind_ok {
                        continue;
                    }
                    let (neighbor, is_outbound) = if edge.from == current_id {
                        (edge.to, true)
                    } else if edge.to == current_id {
                        (edge.from, false)
                    } else {
                        continue;
                    };
                    let dir_ok = match direction_str {
                        "out" => is_outbound,
                        "in" => !is_outbound,
                        _ => true,
                    };
                    if dir_ok && !visited.contains(&neighbor) {
                        queue.push_back((neighbor, d + 1));
                    }
                }
            }

            collected_ids
                .iter()
                .filter_map(|id| store.get_indexed(id).ok().flatten())
                .filter(|t| !t.deleted)
                .collect()
        };

        // Build lookup sets for edge checks.
        let ticket_ids: HashSet<Uuid> = tickets.iter().map(|t| t.id).collect();
        let done_states: HashSet<&str> = ["done", "cancelled"].into_iter().collect();

        let done_ids: HashSet<Uuid> = tickets
            .iter()
            .filter(|t| {
                t.state
                    .as_deref()
                    .map(|s| done_states.contains(s))
                    .unwrap_or(false)
            })
            .map(|t| t.id)
            .collect();

        let mut unresolved_deps: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for edge in &all_edges {
            if edge.kind == "depends_on" && ticket_ids.contains(&edge.from) {
                if !done_ids.contains(&edge.to) {
                    unresolved_deps.entry(edge.from).or_default().push(edge.to);
                }
            }
        }

        let mut findings: Vec<Value> = Vec::new();
        let mut summary: BTreeMap<&str, u64> = BTreeMap::new();

        for t in &tickets {
            if done_ids.contains(&t.id) {
                continue;
            }
            let short_id = &t.id.to_string()[..8];
            let title = t.title.as_deref().unwrap_or("?");

            // 1. Missing description file.
            let desc = TicketFs::read_description(&t.path);
            if desc.is_none() {
                *summary.entry("missing_description").or_insert(0) += 1;
                findings.push(serde_json::json!({
                    "ticket_id": t.id, "short_id": short_id, "title": title,
                    "check": "missing_description", "severity": "warning",
                    "message": "No description.md file — ticket lacks detailed context.",
                }));
            } else if let Some(ref body) = desc {
                let trimmed_len = body.trim().len();
                if trimmed_len < 50 {
                    *summary.entry("short_description").or_insert(0) += 1;
                    findings.push(serde_json::json!({
                        "ticket_id": t.id, "short_id": short_id, "title": title,
                        "check": "short_description", "severity": "info",
                        "message": format!("description.md is very short ({trimmed_len} chars) — consider adding more detail."),
                    }));
                }
            }

            // 3. Missing title.
            if t.title.is_none() || t.title.as_deref() == Some("") {
                *summary.entry("missing_title").or_insert(0) += 1;
                findings.push(serde_json::json!({
                    "ticket_id": t.id, "short_id": short_id, "title": "(none)",
                    "check": "missing_title", "severity": "error",
                    "message": "Ticket has no title.",
                }));
            }

            // 4. Has unresolved deps but not in new state.
            let state = t.state.as_deref().unwrap_or("");
            let has_unresolved = unresolved_deps.contains_key(&t.id);
            if has_unresolved && state != "new" {
                let dep_count = unresolved_deps[&t.id].len();
                *summary.entry("unblocked_with_deps").or_insert(0) += 1;
                findings.push(serde_json::json!({
                    "ticket_id": t.id, "short_id": short_id, "title": title,
                    "check": "unblocked_with_deps", "severity": "info",
                    "message": format!("Ticket is '{state}' but has {dep_count} unresolved dependency/ies — may need state review."),
                }));
            }

            // 6. Dangling dependency edges.
            for edge in &all_edges {
                if edge.from == t.id && edge.kind == "depends_on" {
                    let target_exists = store
                        .get_indexed(&edge.to)
                        .ok()
                        .flatten()
                        .map(|tgt| !tgt.deleted)
                        .unwrap_or(false);
                    if !target_exists {
                        let target_short = &edge.to.to_string()[..8];
                        *summary.entry("dangling_edge").or_insert(0) += 1;
                        findings.push(serde_json::json!({
                            "ticket_id": t.id, "short_id": short_id, "title": title,
                            "check": "dangling_edge", "severity": "error",
                            "message": format!("depends_on edge points to {target_short} which is deleted or missing."),
                        }));
                    }
                }
            }
        }

        let total_checked = tickets.iter().filter(|t| !done_ids.contains(&t.id)).count();

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "tickets_checked": total_checked,
            "finding_count": findings.len(),
            "summary": summary,
            "findings": findings,
        }))
        }).await
    }
}

#[tool_router]
impl TicketServer {
    #[tool(name = "health", description = "Check that the ticket store is accessible.")]
    async fn health(&self) -> Result<CallToolResult, McpError> {
        match self.with_store(|store| store.list(None, None, Some(0))).await {
            Ok(_) => Self::json_result(&serde_json::json!({
                "status": "ok",
                "service": "ticket-mcp",
                "mode": "direct",
            })),
            Err(e) => Self::json_result(&serde_json::json!({
                "status": "error",
                "error": e.to_string(),
            })),
        }
    }

    #[tool(
        name = "list_workspaces",
        description = "List available ticket workspaces."
    )]
    async fn list_workspaces(&self) -> Result<CallToolResult, McpError> {
        let config = ticket_api::workspace::WorkspaceConfig::load();
        let names: Vec<String> = if config.workspaces.is_empty() {
            vec!["default".to_string()]
        } else {
            config.workspaces.keys().cloned().collect()
        };
        Self::json_result(&serde_json::json!({
            "workspaces": names,
            "active": config.active,
        }))
    }

    #[tool(
        name = "list_tickets",
        description = "List tickets with optional state/query/limit filters."
    )]
    async fn list_tickets(
        &self,
        Parameters(input): Parameters<ListTicketsInput>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(q) = &input.query {
            let limit = input.limit.unwrap_or(100).min(1000);
            let items: Vec<TicketSummary> = self.with_store(|store| {
                let results = store.search_tickets(q, limit)?;
                Ok(results
                    .into_iter()
                    .map(|r| {
                        let updated_at = store
                            .get_indexed(&r.id)
                            .ok()
                            .flatten()
                            .map(|t| t.updated_at)
                            .unwrap_or_else(|| {
                                chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::UNIX_EPOCH)
                            });
                        TicketSummary {
                            id: r.id.to_string(),
                            type_id: r.ticket_type.unwrap_or_default(),
                            title: r.title,
                            state: r.state,
                            updated_at,
                        }
                    })
                    .collect())
            }).await?;
            Self::json_result(&serde_json::json!({
                "workspace": input.workspace,
                "items": items,
            }))
        } else {
            let limit = input.limit.map(|l| l.min(1000));
            let items: Vec<TicketSummary> = self.with_store(|store| {
                Ok(store
                    .list(input.state.as_deref(), None, limit)?
                    .into_iter()
                    .map(|t| TicketSummary {
                        id: t.id.to_string(),
                        type_id: t.type_id,
                        title: t.title,
                        state: t.state,
                        updated_at: t.updated_at,
                    })
                    .collect())
            }).await?;
            Self::json_result(&serde_json::json!({
                "workspace": input.workspace,
                "items": items,
            }))
        }
    }

    #[tool(name = "get_ticket", description = "Get one ticket by id.")]
    async fn get_ticket(
        &self,
        Parameters(input): Parameters<TicketRefInput>,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        self.with_store_ext(move |store| {
            let id = Self::resolve_uuid_with(store, &input.id)?;
            let manifest = store.get(&id).map_err(Self::store_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "ticket": TicketDetail {
                    id: manifest.id.to_string(),
                    created_at: manifest.created_at,
                    fields: manifest.extra,
                },
            }))
        }).await
    }

    #[tool(
        name = "get_ticket_description",
        description = "Get ticket markdown description by id."
    )]
    async fn get_ticket_description(
        &self,
        Parameters(input): Parameters<TicketRefInput>,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        self.with_store_ext(move |store| {
            let id = Self::resolve_uuid_with(store, &input.id)?;
            let indexed = store.get_indexed(&id).map_err(Self::store_err)?
                .ok_or_else(|| McpError::invalid_params(format!("ticket not found: {id}"), None))?;

            if indexed.deleted {
                return Err(McpError::invalid_params(format!("ticket deleted: {id}"), None));
            }

            let description = TicketFs::read_description(&indexed.path);
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "id": id.to_string(),
                "description": description,
            }))
        }).await
    }

    #[tool(
        name = "list_edges",
        description = "List ticket graph edges, optionally filtered by edge kind."
    )]
    async fn list_edges(
        &self,
        Parameters(input): Parameters<ListEdgesInput>,
    ) -> Result<CallToolResult, McpError> {
        let all = self.with_store(|store| store.list_all_edges()).await?;
        let items: Vec<EdgeItem> = all
            .into_iter()
            .filter(|e| match &input.kind {
                Some(k) => k == "all" || e.kind == *k,
                None => true,
            })
            .map(|e| EdgeItem {
                from: e.from.to_string(),
                to: e.to.to_string(),
                kind: e.kind,
            })
            .collect();
        Self::json_result(&serde_json::json!({
            "workspace": input.workspace,
            "items": items,
        }))
    }

    #[tool(
        name = "subgraph",
        description = "Fetch dependency subgraph for a root ticket via BFS traversal."
    )]
    async fn subgraph(
        &self,
        Parameters(input): Parameters<SubgraphInput>,
    ) -> Result<CallToolResult, McpError> {
        self.bfs_graph(
            input.workspace,
            &input.root,
            input.direction.as_deref().unwrap_or("both"),
            input.edge_kind.as_deref(),
            input.depth,
            input.limit_nodes,
            input.limit_edges,
        ).await
    }

    #[tool(
        name = "topgraph",
        description = "Fetch reverse dependency graph (tickets that depend on the root) via BFS traversal."
    )]
    async fn topgraph(
        &self,
        Parameters(input): Parameters<TopgraphInput>,
    ) -> Result<CallToolResult, McpError> {
        self.bfs_graph(
            input.workspace,
            &input.root,
            input.direction.as_deref().unwrap_or("in"),
            input.edge_kind.as_deref(),
            input.depth,
            input.limit_nodes,
            input.limit_edges,
        ).await
    }

    #[tool(
        name = "next_tickets",
        description = "List unblocked tickets in any non-terminal state whose dependencies are all satisfied, ordered by workflow progress (closest to done first), then priority. Designed for worker agents to pick the next implementable item."
    )]
    async fn next_tickets(
        &self,
        Parameters(input): Parameters<NextTicketsInput>,
    ) -> Result<CallToolResult, McpError> {
        let limit = input.limit.unwrap_or(20).min(100);
        let filter = input.filter.clone();

        let items: Vec<Value> = self.with_store(|store| {
            let all = store.list(None, None, None)?;

            let tickets: Vec<_> = if let Some(ref prefix) = filter {
                all.into_iter()
                    .filter(|t| {
                        t.title
                            .as_deref()
                            .unwrap_or("")
                            .starts_with(prefix.as_str())
                    })
                    .collect()
            } else {
                all
            };

            let done_states: &[&str] = &["done", "cancelled"];

            let done_ids: HashSet<Uuid> = tickets
                .iter()
                .filter(|t| {
                    t.state
                        .as_deref()
                        .map(|s| done_states.contains(&s))
                        .unwrap_or(false)
                })
                .map(|t| t.id)
                .collect();

            let all_edges = store.list_all_edges()?;
            let mut blockers: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
            for edge in &all_edges {
                if edge.kind == "depends_on" && !done_ids.contains(&edge.to) {
                    blockers.entry(edge.from).or_default().push(edge.to);
                }
            }

            // Build state-index map for progress sorting.
            let mut state_index: HashMap<String, usize> = HashMap::new();
            for type_id in store.schema_registry().type_ids() {
                if let Some(schema) = store.schema_registry().get(type_id) {
                    for (i, s) in schema.states.iter().enumerate() {
                        state_index.entry(s.clone()).or_insert(i);
                    }
                }
            }

            let mut candidates: Vec<_> = tickets
                .iter()
                .filter(|t| {
                    t.state
                        .as_deref()
                        .map(|s| !done_states.contains(&s))
                        .unwrap_or(true)
                })
                .filter(|t| blockers.get(&t.id).map_or(true, |b| b.is_empty()))
                .collect();

            // Read priority for sorting.
            let mut priority_map: HashMap<Uuid, String> = HashMap::new();
            for t in &candidates {
                if let Ok(manifest) = TicketFs::read(&t.path) {
                    if let Some(p) = manifest.extra.get("priority").and_then(|v| v.as_str()) {
                        priority_map.insert(t.id, p.to_string());
                    }
                }
            }

            let priority_weight = |p: &str| -> u8 {
                match p {
                    "critical" => 0,
                    "high" => 1,
                    "medium" => 2,
                    "low" => 3,
                    "backlog" => 5,
                    _ => 4, // none / unset
                }
            };

            // Sort by state progress (highest index first), then priority, then oldest.
            candidates.sort_by(|a, b| {
                let sa = a.state.as_deref().unwrap_or("");
                let sb = b.state.as_deref().unwrap_or("");
                let si_a = state_index.get(sa).copied().unwrap_or(0);
                let si_b = state_index.get(sb).copied().unwrap_or(0);
                si_b.cmp(&si_a)
                    .then_with(|| {
                        let pa = priority_map.get(&a.id).map(|s| s.as_str()).unwrap_or("");
                        let pb = priority_map.get(&b.id).map(|s| s.as_str()).unwrap_or("");
                        priority_weight(pa).cmp(&priority_weight(pb))
                    })
                    .then_with(|| a.created_at.cmp(&b.created_at))
            });

            candidates.truncate(limit);

            Ok(candidates
                .iter()
                .enumerate()
                .map(|(rank, t)| {
                    let prio = priority_map
                        .get(&t.id)
                        .cloned()
                        .unwrap_or_else(|| "none".to_string());
                    serde_json::json!({
                        "rank": rank + 1,
                        "id": t.id.to_string(),
                        "title": t.title,
                        "state": t.state,
                        "type": t.type_id,
                        "priority": prio,
                        "created_at": t.created_at.to_rfc3339(),
                    })
                })
                .collect())
        }).await?;

        Self::json_result(&serde_json::json!({
            "workspace": input.workspace,
            "count": items.len(),
            "items": items,
        }))
    }

    #[tool(
        name = "health_check",
        description = "Run health checks on tickets: validates descriptions, titles, dependency state consistency, and dangling edges. Scope by root (BFS subgraph), explicit IDs, or all tickets."
    )]
    async fn health_check(
        &self,
        Parameters(input): Parameters<HealthCheckInput>,
    ) -> Result<CallToolResult, McpError> {
        self.run_health_checks(
            &input.workspace,
            input.root.as_deref(),
            input.all,
            &input.ids,
            input.depth,
            input.direction.as_deref(),
        ).await
    }

    #[tool(
        name = "update_ticket",
        description = "Update a ticket: apply field patches and/or transition state. Set undo=true to revert to the previous history revision."
    )]
    async fn update_ticket(
        &self,
        Parameters(input): Parameters<UpdateTicketInput>,
    ) -> Result<CallToolResult, McpError> {
        if input.undo {
            if input.to_state.is_some() || !input.fields.is_empty() {
                return Err(McpError::invalid_params(
                    "undo cannot be combined with to_state or fields",
                    None,
                ));
            }
            let workspace = input.workspace;
            let id_str = input.id;
            let (prev_rev, new_rev, updated) = self.with_store_ext(move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                let revisions = store.get_history(&id).map_err(Self::store_err)?;
                if revisions.len() < 2 {
                    return Err(Self::store_err(ticket_api::error::StorageError::Database(
                        "cannot undo: not enough history revisions".into(),
                    )));
                }
                let prev = &revisions[revisions.len() - 2];
                let prev_rev = prev.rev;
                let new_rev = store.apply_revert(&id, prev.fields.clone()).map_err(Self::store_err)?;
                let updated = store.get(&id).map_err(Self::store_err)?;
                Ok((prev_rev, new_rev, updated))
            }).await?;
            return Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "undo": true,
                "reverted_to": prev_rev,
                "new_rev": new_rev,
                "ticket": TicketDetail {
                    id: updated.id.to_string(),
                    created_at: updated.created_at,
                    fields: updated.extra,
                },
            }));
        }

        let mut patch = BTreeMap::new();
        for raw in &input.fields {
            let (k, v) = raw.split_once('=').ok_or_else(|| {
                McpError::invalid_params(format!("invalid field format '{raw}', expected key=value"), None)
            })?;
            patch.insert(k.trim().to_string(), Value::String(v.trim().to_string()));
        }
        let workspace = input.workspace;
        let to_state = input.to_state;
        let id_str = input.id;
        let manifest = self.with_store_ext(move |store| {
            let id = Self::resolve_uuid_with(store, &id_str)?;
            store.update(&id, patch, None, to_state.as_deref()).map_err(Self::store_err)
        }).await?;
        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "ticket": TicketDetail {
                id: manifest.id.to_string(),
                created_at: manifest.created_at,
                fields: manifest.extra,
            },
        }))
    }

    #[tool(
        name = "close_ticket",
        description = "Fast-forward a ticket to a target state by traversing all intermediate transitions (default: done)."
    )]
    async fn close_ticket(
        &self,
        Parameters(input): Parameters<CloseTicketInput>,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let to_state = input.to_state;
        let id_str = input.id;
        let target_state = to_state.clone();
        let (manifest, path) = self.with_store_ext(move |store| {
            let id = Self::resolve_uuid_with(store, &id_str)?;
            store.close(&id, &to_state).map_err(Self::store_err)
        }).await?;
        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "id": manifest.id.to_string(),
            "target_state": target_state,
            "traversed_states": path,
        }))
    }

    #[tool(
        name = "cancel_ticket",
        description = "Cancel a ticket (fast-forward to 'cancelled' state)."
    )]
    async fn cancel_ticket(
        &self,
        Parameters(input): Parameters<CancelTicketInput>,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let (manifest, path) = self.with_store_ext(move |store| {
            let id = Self::resolve_uuid_with(store, &id_str)?;
            store.close(&id, "cancelled").map_err(Self::store_err)
        }).await?;
        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "id": manifest.id.to_string(),
            "traversed_states": path,
        }))
    }

    #[tool(
        name = "workflow",
        description = "Show ready-to-run ticket MCP call sequences for common tasks."
    )]
    async fn workflow(
        &self,
        Parameters(input): Parameters<WorkflowInput>,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.unwrap_or_else(|| "default".to_string());
        let id = input.id.unwrap_or_else(|| "<ticket-id>".to_string());
        let query = input.query.unwrap_or_else(|| "<query>".to_string());

        let payload = match input.name {
            WorkflowName::List => serde_json::json!({
                "available": [
                    "triage_open_tickets",
                    "fetch_ticket_context",
                    "inspect_dependencies"
                ],
                "note": "Use one of the named workflows to get an ordered sequence of tool calls."
            }),
            WorkflowName::TriageOpenTickets => serde_json::json!({
                "name": "triage_open_tickets",
                "steps": [
                    {"tool": "health", "input": {}},
                    {"tool": "list_workspaces", "input": {}},
                    {"tool": "list_tickets", "input": {"workspace": workspace, "state": "new", "limit": 50}},
                    {"tool": "list_tickets", "input": {"workspace": workspace, "state": "in-implementation", "limit": 50}}
                ]
            }),
            WorkflowName::FetchTicketContext => serde_json::json!({
                "name": "fetch_ticket_context",
                "steps": [
                    {"tool": "get_ticket", "input": {"workspace": workspace, "id": id}},
                    {"tool": "get_ticket_description", "input": {"workspace": workspace, "id": id}},
                    {"tool": "list_edges", "input": {"workspace": workspace}},
                    {"tool": "subgraph", "input": {"workspace": workspace, "root": id, "depth": 2}}
                ]
            }),
            WorkflowName::InspectDependencies => serde_json::json!({
                "name": "inspect_dependencies",
                "steps": [
                    {"tool": "list_tickets", "input": {"workspace": workspace, "query": query, "limit": 20}},
                    {"tool": "list_edges", "input": {"workspace": workspace, "kind": "depends_on"}},
                    {"tool": "subgraph", "input": {"workspace": workspace, "root": id, "direction": "both", "depth": 3}}
                ]
            }),
        };

        Self::json_result(&payload)
    }

    #[tool(
        name = "help",
        description = "List ticket-mcp tools and their parameters."
    )]
    async fn help(&self) -> Result<CallToolResult, McpError> {
        let payload = serde_json::json!({
            "mode": "direct (no HTTP backend required)",
            "tools": [
                "health",
                "list_workspaces",
                "list_tickets",
                "get_ticket",
                "get_ticket_description",
                "list_edges",
                "subgraph",
                "topgraph",
                "health_check",
                "update_ticket",
                "close_ticket",
                "cancel_ticket",
                "workflow",
            ],
            "operations": {
                "health": {
                    "description": "Check store is accessible",
                    "required": [],
                },
                "list_workspaces": {
                    "description": "List available workspaces",
                    "required": [],
                },
                "list_tickets": {
                    "description": "List/search tickets",
                    "required": ["workspace"],
                    "optional": ["state", "query", "limit"],
                },
                "get_ticket": {
                    "description": "Get full ticket manifest",
                    "required": ["workspace", "id"],
                },
                "get_ticket_description": {
                    "description": "Get ticket markdown description",
                    "required": ["workspace", "id"],
                },
                "list_edges": {
                    "description": "List graph edges",
                    "required": ["workspace"],
                    "optional": ["kind"],
                },
                "subgraph": {
                    "description": "BFS dependency subgraph",
                    "required": ["workspace", "root"],
                    "optional": ["direction", "edge_kind", "depth", "limit_nodes", "limit_edges"],
                },
                "topgraph": {
                    "description": "BFS reverse dependency graph",
                    "required": ["workspace", "root"],
                    "optional": ["direction", "edge_kind", "depth", "limit_nodes", "limit_edges"],
                },
                "health_check": {
                    "description": "Run health checks on tickets (descriptions, titles, deps, edges)",
                    "required": ["workspace"],
                    "optional": ["root", "all", "ids", "depth", "direction"],
                },
                "next_tickets": {
                    "description": "List unblocked ready tickets in priority order for worker agents",
                    "required": ["workspace"],
                    "optional": ["limit", "filter"],
                },
                "update_ticket": {
                    "description": "Update ticket fields and/or transition state",
                    "required": ["workspace", "id"],
                    "optional": ["to_state", "fields"],
                },
                "close_ticket": {
                    "description": "Fast-forward ticket to target state",
                    "required": ["workspace", "id"],
                    "optional": ["to_state"],
                },
                "cancel_ticket": {
                    "description": "Cancel a ticket",
                    "required": ["workspace", "id"],
                },
            },
            "notes": [
                "Direct store access — no HTTP backend required.",
                "Set TICKET_INDEX_ROOT to override workspace resolution.",
            ],
        });
        Self::json_result(&payload)
    }
}

#[tool_handler]
impl ServerHandler for TicketServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "ticket-mcp provides direct access to the ticket store. No HTTP backend required. Use named tools for ticket operations."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub async fn run_mcp_server(
    index_root: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = TicketServer::new(index_root);

    tracing::info!("Starting ticket-mcp server on stdio (direct store access)");

    let service = server.serve(stdio()).await.inspect_err(|err| {
        eprintln!("Server error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}
