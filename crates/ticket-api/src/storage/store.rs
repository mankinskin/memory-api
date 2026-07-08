use std::{
    collections::{
        BTreeMap,
        BTreeSet,
        HashMap,
    },
    fs,
    path::{
        Path,
        PathBuf,
    },
    sync::OnceLock,
    time::Instant,
};

use chrono::{
    DateTime,
    Utc,
};
use memory_api::{
    model::filesystem::ScanRoot,
    storage::ensure_sqlite_index_root,
};
use serde_json::Value;
use tracing::field::Empty;
use uuid::Uuid;

use crate::{
    error::StorageError,
    model::{
        filesystem::TICKET_MANIFEST_FILE,
        schema_registry::SchemaRegistry,
        ticket::{
            TicketId,
            TicketManifest,
        },
    },
    storage::{
        index::RedbIndexStore,
        indexed::IndexedTicket,
        search::TantivySearchIndex,
        ticket_fs::TicketFs,
    },
    workspace,
};

mod board;
mod lifecycle;
mod query;
mod release;
mod scan;
mod workflow_facts;
mod store_open;

pub use self::{
    release::{
        GateCheckOutcome,
        GateStatus,
        PromoteOutcome,
        ValidationResultOutcome,
    },
    scan::ScanReport,
};

const STORE_TRACE_TARGET: &str = "ticket_api::storage::store";

#[derive(Debug, Clone, Default)]
pub struct StoreOpenReport {
    pub initialized_store: bool,
    pub phase_timings_ms: BTreeMap<String, u64>,
    pub scan_reports: BTreeMap<String, ScanReport>,
}

/// Trait for receiving mutation events from the store (e.g. for SSE streaming).
///
/// Implement this in the HTTP layer and attach it via [`TicketStore::set_hook`].
pub trait StoreHook: Send + Sync + 'static {
    fn ticket_upsert(
        &self,
        id: Uuid,
        state: Option<String>,
        title: Option<String>,
        updated_at: chrono::DateTime<chrono::Utc>,
    );
    fn ticket_delete(
        &self,
        id: Uuid,
    );
    fn edge_upsert(
        &self,
        from: Uuid,
        to: Uuid,
        kind: String,
    );
    fn edge_delete(
        &self,
        from: Uuid,
        to: Uuid,
        kind: String,
    );
}

/// The central ticket store: filesystem source-of-truth + SQLite metadata index +
/// Tantivy full-text search index.
///
/// Ticket manifests persist graph edges in file-backed fields such as
/// `depends_on` and `linked`, while SQLite caches the queryable edge table.
/// A forced scan backfills those file-backed edge fields for legacy stores and
/// then rebuilds the cached edge table from the tracked manifests.
pub struct TicketStore {
    index: RedbIndexStore,
    search: TantivySearchIndex,
    schema_registry: SchemaRegistry,
    /// Root directory for the SQLite database and Tantivy index files.
    pub index_root: PathBuf,
    /// Optional mutation hook. Set by the HTTP layer when streaming is active.
    /// Not used in CLI mode.
    hook: OnceLock<Box<dyn StoreHook>>,
}

const FILE_BACKED_EDGE_FIELDS: &[&str] = &["depends_on", "linked"];

impl TicketStore {
    /// Attach a mutation hook. May only be called once; subsequent calls
    /// are silently ignored (the first hook wins).
    pub fn set_hook(
        &self,
        hook: impl StoreHook,
    ) {
        let _ = self.hook.set(Box::new(hook));
    }

    /// Return a reference to the hook if one has been set.
    fn hook(&self) -> Option<&dyn StoreHook> {
        self.hook.get().map(|b| b.as_ref())
    }

    pub(crate) fn with_search_repair<T, F>(
        &self,
        mut op: F,
    ) -> Result<T, StorageError>
    where
        F: FnMut() -> Result<T, StorageError>,
    {
        // Proactively enforce the search-index structural invariants before a
        // write instead of catching a failure afterwards. A rebuild leaves the
        // index empty; the completeness invariant is restored by re-indexing the
        // on-disk tickets. Writes use the structural (rebuild-only) check rather
        // than the document-count check so an in-progress mutation that has
        // already updated the metadata index does not trigger a full reindex.
        if self.search.heal_if_needed()? {
            self.scan(true)?;
        }
        op()
    }

    /// Enforce the search-index completeness invariant before a read.
    ///
    /// Heals structural corruption (via [`TantivySearchIndex::num_docs`]) and
    /// repopulates the index from the on-disk tickets when its document count
    /// does not match the metadata index — the filesystem-backed source of
    /// truth that survives Tantivy corruption.
    pub(crate) fn ensure_search_complete(&self) -> Result<(), StorageError> {
        if self.search_needs_rebuild()? {
            self.scan(true)?;
        }
        Ok(())
    }

    /// Whether the search index must be rebuilt before it can be trusted.
    ///
    /// Returns `true` when the index cannot be opened/counted (structural or
    /// segment-content corruption) or when its document count differs from the
    /// metadata index. Calling this also heals the cheap structural invariants.
    fn search_needs_rebuild(&self) -> Result<bool, StorageError> {
        let indexed = self.index.list_tickets()?.len() as u64;
        match self.search.num_docs() {
            Ok(docs) => Ok(docs != indexed),
            Err(_) => Ok(true),
        }
    }

    pub fn schema_registry(&self) -> &SchemaRegistry {
        &self.schema_registry
    }

    // ── ticket CRUD ──────────────────────────────────────────────────────────

    /// Create a new ticket.
    ///
    /// `target_root`: a registered scan root, workspace root, store root, or
    /// path inside a local `.ticket/` store. If `None`, the first registered
    /// scan root is used (error if none exist).
    pub fn create(
        &self,
        id: Option<Uuid>,
        type_id: &str,
        title: Option<&str>,
        initial_state: Option<&str>,
        extra: BTreeMap<String, Value>,
        target_root: Option<&Path>,
        body: Option<&str>,
    ) -> Result<TicketId, StorageError> {
        let id = id.unwrap_or_else(Uuid::new_v4);
        let now = Utc::now();

        // Resolve target scan root.
        let root = self.resolve_target_root(target_root)?;
        std::fs::create_dir_all(&root)?;

        let mut manifest = TicketManifest::new(id, now);
        manifest
            .extra
            .insert("type".to_string(), Value::String(type_id.to_string()));
        if let Some(t) = title {
            manifest
                .extra
                .insert("title".to_string(), Value::String(t.to_string()));
        }
        let state = initial_state.unwrap_or("new").to_string();
        manifest
            .extra
            .insert("state".to_string(), Value::String(state.clone()));
        for (k, v) in extra {
            manifest.extra.insert(k, v);
        }

        // Validate against type schema if known.
        if let Some(schema) = self.schema_registry.get(type_id) {
            schema.validate_manifest(&manifest)?;
        }

        let ticket_path = Self::normalize_existing_path(&TicketFs::create(
            &manifest, &root, body,
        )?);

        let indexed = IndexedTicket {
            id,
            path: ticket_path,
            type_id: type_id.to_string(),
            title: title.map(str::to_string),
            state: Some(state.clone()),
            created_at: now,
            updated_at: now,
        };
        self.index.insert_ticket(&indexed)?;

        // Use the provided body directly (already written to disk); fall back to
        // reading the file for scan-integrated tickets that may have existing content.
        let body_for_index = body
            .map(str::to_string)
            .or_else(|| TicketFs::read_description(&indexed.path));
        let created_at_str = indexed.created_at.to_rfc3339();
        let effort_str = manifest.extra.get("effort")
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
        self.with_search_repair(|| {
            self.search.upsert(
                &id,
                title,
                body_for_index.as_deref(),
                Some(&state),
                Some(type_id),
                Some(&created_at_str),
                effort_str.as_deref(),
            )
        })?;

        // Append initial history snapshot (rev 1).
        let _ = TicketFs::append_history(
            &indexed.path,
            manifest.extra.clone(),
            None,
        );

        // Emit SSE hook event.
        if let Some(h) = self.hook() {
            h.ticket_upsert(
                id,
                Some(state),
                title.map(str::to_string),
                indexed.updated_at,
            );
        }

        self.refresh_workflow_facts_for_roots(&[id], false, now)?;

        Ok(id)
    }

    fn resolve_target_root(
        &self,
        target_root: Option<&Path>,
    ) -> Result<PathBuf, StorageError> {
        let Some(target_root) = target_root else {
            // Canonical: write into the workspace's own .ticket/tickets/
            // directory (resolved via the index_root), ignoring any registered
            // scan roots. Callers that want to place tickets elsewhere must
            // pass an explicit `target_root`.
            return Ok(self.index_root.join("tickets"));
        };

        let roots = self.list_scan_roots()?;

        let requested = if target_root.is_dir() {
            target_root.to_path_buf()
        } else {
            target_root.parent().unwrap_or(target_root).to_path_buf()
        };
        let requested = self.resolve_scan_root_path(&requested);

        if let Some(root) = roots
            .iter()
            .find(|root| root.path == requested)
            .map(|root| root.path.clone())
        {
            return Ok(root);
        }

        let store_root = workspace::resolve_store_root_from(
            target_root,
            workspace::TICKET_INDEX_DIR,
        );
        if store_root.file_name().and_then(|name| name.to_str())
            == Some(workspace::TICKET_INDEX_DIR)
        {
            return Ok(self.resolve_scan_root_path(&store_root.join("tickets")));
        }

        Err(StorageError::Other(format!(
            "invalid ticket root '{}': expected a registered scan root, a workspace root containing .ticket, the .ticket store itself, or a path inside that store",
            target_root.display()
        )))
    }

    /// Read the full manifest for a ticket by ID.
    pub fn get(
        &self,
        id: &Uuid,
    ) -> Result<TicketManifest, StorageError> {
        let indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        TicketFs::read(&indexed.path)
    }

    /// Get just the indexed metadata (faster than a full read).
    pub fn get_indexed(
        &self,
        id: &Uuid,
    ) -> Result<Option<IndexedTicket>, StorageError> {
        Ok(self
            .index
            .get_ticket(id)?
            .map(|ticket| self.normalize_indexed_ticket(ticket)))
    }

    /// Fetch multiple tickets by ID in a single ReDB read transaction.
    ///
    /// Returns a `HashMap<Uuid, IndexedTicket>` for O(1) lookup. Missing
    /// IDs are omitted. Prefer this over N separate `get_indexed()`
    /// calls when you need metadata for a known set of IDs (e.g. BFS nodes).
    pub fn get_indexed_many(
        &self,
        ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, IndexedTicket>, StorageError>
    {
        Ok(self
            .index
            .get_tickets_by_ids(ids)?
            .into_iter()
            .map(|(id, ticket)| (id, self.normalize_indexed_ticket(ticket)))
            .collect())
    }

    pub fn get_workflow_facts(
        &self,
        id: &Uuid,
    ) -> Result<Option<crate::storage::indexed::WorkflowFacts>, StorageError> {
        self.index.get_workflow_facts(id)
    }

    pub fn get_workflow_facts_many(
        &self,
        ids: &[Uuid],
    ) -> Result<HashMap<Uuid, crate::storage::indexed::WorkflowFacts>, StorageError> {
        self.index.get_workflow_facts_many(ids)
    }

    /// Update a ticket: apply field patches, optional state transition, and optional description.
    pub fn update(
        &self,
        id: &Uuid,
        patch: BTreeMap<String, Value>,
        transition_states: Option<&[String]>,
        to_state: Option<&str>,
        description: Option<&str>,
        author: Option<&str>,
    ) -> Result<TicketManifest, StorageError> {
        let mut patch = patch;

        let mut indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        let current_manifest = TicketFs::read(&indexed.path)?;
        let edge_patch_plans = edge_patch_plans(&patch, &current_manifest.extra)?;
        strip_file_backed_edge_fields(&mut patch);

        // Determine the target state and transition path.
        // Priority: to_state > last element of transition_states > current state (no change)
        let (new_state, transition_path) = self.resolve_update_target(
            &indexed,
            transition_states,
            to_state,
        )?;
        let previous_state = indexed.state.clone();
        let updated_manifest = self.apply_manifest_update(
            &indexed.path,
            &patch,
            &new_state,
            &transition_path,
            description,
        )?;

        // Route edge-field updates through canonical graph APIs.
        apply_edge_patch_plans(self, *id, edge_patch_plans)?;

        // Refresh indexed metadata.
        let now = Utc::now();
        self.refresh_index_and_search(
            id,
            &patch,
            &new_state,
            &updated_manifest,
            &mut indexed,
            now,
        )?;

        // Append history snapshot after successful write.
        let _ = TicketFs::append_history(
            &indexed.path,
            updated_manifest.extra.clone(),
            author.map(str::to_string),
        );

        // Emit SSE hook event.
        if let Some(h) = self.hook() {
            h.ticket_upsert(
                *id,
                indexed.state.clone(),
                indexed.title.clone(),
                indexed.updated_at,
            );
        }

        // Reconcile board: mark completed on terminal states.
        self.board_reconcile(id, false);

        let state_progressed = previous_state.as_deref() != new_state.as_deref()
            && self.state_rank_for_type(&indexed.type_id, new_state.as_deref())
                > self.state_rank_for_type(&indexed.type_id, previous_state.as_deref());
        if previous_state.as_deref() != new_state.as_deref() {
            self.refresh_workflow_facts_for_roots(&[*id], state_progressed, now)?;
        }

        Ok(TicketFs::read(&indexed.path).unwrap_or(updated_manifest))
    }

    fn resolve_update_target(
        &self,
        indexed: &IndexedTicket,
        transition_states: Option<&[String]>,
        to_state: Option<&str>,
    ) -> Result<(Option<String>, Vec<String>), StorageError> {
        if let Some(to) = to_state {
            let path = self.resolve_transition_path(
                indexed,
                transition_states.unwrap_or(&[]),
                to,
            )?;
            let final_state = path
                .last()
                .cloned()
                .unwrap_or_else(|| to.to_string());
            return Ok((Some(final_state), path));
        }

        if let Some(transition_states_slice) = transition_states {
            if let Some(final_target) = transition_states_slice.last() {
                let intermediate_steps = &transition_states_slice[..transition_states_slice.len() - 1];
                let path = self.resolve_transition_path(
                    indexed,
                    intermediate_steps,
                    final_target,
                )?;
                return Ok((Some(final_target.clone()), path));
            }
            return Ok((indexed.state.clone(), Vec::new()));
        }

        Ok((indexed.state.clone(), Vec::new()))
    }

    fn apply_manifest_update(
        &self,
        ticket_path: &Path,
        patch: &BTreeMap<String, Value>,
        new_state: &Option<String>,
        transition_path: &[String],
        description: Option<&str>,
    ) -> Result<TicketManifest, StorageError> {
        let updated_manifest = if transition_path.is_empty() {
            TicketFs::update(ticket_path, patch, new_state.as_deref())?
        } else {
            let mut manifest = None;
            for (index, state) in transition_path.iter().enumerate() {
                let step_patch = if index + 1 == transition_path.len() {
                    patch.clone()
                } else {
                    BTreeMap::new()
                };
                manifest = Some(TicketFs::update(
                    ticket_path,
                    &step_patch,
                    Some(state.as_str()),
                )?);
            }
            manifest.expect("transition path produces at least one manifest")
        };

        if let Some(desc) = description {
            TicketFs::write_description(ticket_path, desc)?;
        }

        Ok(updated_manifest)
    }

    fn refresh_index_and_search(
        &self,
        id: &Uuid,
        patch: &BTreeMap<String, Value>,
        new_state: &Option<String>,
        updated_manifest: &TicketManifest,
        indexed: &mut IndexedTicket,
        now: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        indexed.updated_at = now;
        if let Some(s) = new_state {
            indexed.state = Some(s.clone());
        }
        if let Some(title_val) = patch.get("title").and_then(|v| v.as_str()) {
            indexed.title = Some(title_val.to_string());
        }
        self.index.insert_ticket(indexed)?;

        let body = TicketFs::read_description(&indexed.path);
        let created_at_str = indexed.created_at.to_rfc3339();
        let effort_str = updated_manifest
            .extra
            .get("effort")
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
        self.with_search_repair(|| {
            self.search.upsert(
                id,
                indexed.title.as_deref(),
                body.as_deref(),
                indexed.state.as_deref(),
                Some(indexed.type_id.as_str()),
                Some(&created_at_str),
                effort_str.as_deref(),
            )
        })?;

        Ok(())
    }

    fn resolve_transition_path(
        &self,
        indexed: &IndexedTicket,
        transition_states: &[String],
        target_state: &str,
    ) -> Result<Vec<String>, StorageError> {
        let current_state = indexed.state.as_deref().unwrap_or("new");
        if current_state == target_state && transition_states.is_empty() {
            return Ok(vec![]);
        }
        let schema = self.schema_registry.get(&indexed.type_id).ok_or_else(|| {
            StorageError::Other(format!("no schema for type '{}'", indexed.type_id))
        })?;

        let mut path = Vec::new();
        let mut from = current_state.to_string();
        let mut checkpoints: Vec<String> = transition_states.to_vec();
        checkpoints.push(target_state.to_string());

        for checkpoint in checkpoints {
            if checkpoint == from {
                continue;
            }

            let segment = schema.find_path(&from, &checkpoint).ok_or_else(|| {
                StorageError::Other(format!(
                    "no path from '{}' to '{}'",
                    from, checkpoint
                ))
            })?;

            path.extend(segment);
            from = checkpoint;
        }

        if !schema.required_states.is_empty()
            && schema.terminal_states.contains(&target_state.to_string())
        {
            let history = TicketFs::read_history(&indexed.path).unwrap_or_default();
            let mut visited: Vec<String> = history
                .iter()
                .filter_map(|r| {
                    r.fields
                        .get("state")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
                .collect();
            visited.push(current_state.to_string());
            visited.extend(path.iter().cloned());
            schema.validate_workflow(target_state, &visited)?;
        }

        Ok(path)
    }
}

#[derive(Debug)]
struct EdgePatchPlan {
    kind: String,
    to_add: Vec<Uuid>,
    to_remove: Vec<Uuid>,
}


#[path = "store_helpers.rs"]
mod store_helpers;
use store_helpers::*;

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
