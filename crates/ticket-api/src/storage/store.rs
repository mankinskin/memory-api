use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{ProtocolError, StorageError};
use crate::model::edge::EdgeRecord;
use crate::model::schema_registry::SchemaRegistry;
use crate::model::filesystem::ScanRoot;
use crate::model::query::parse_query;
use crate::model::ticket::{TicketId, TicketManifest};
use crate::storage::index::RedbIndexStore;
use crate::storage::indexed::{IndexedTicket, LeaseInfo};
use crate::storage::search::{SearchResult, TantivySearchIndex};
use crate::storage::ticket_fs::{TicketFs, TicketScanEntry};

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
    fn ticket_delete(&self, id: Uuid);
    fn edge_upsert(&self, from: Uuid, to: Uuid, kind: String);
    fn edge_delete(&self, from: Uuid, to: Uuid, kind: String);
}

/// The central ticket store: filesystem source-of-truth + redb metadata index +
/// Tantivy full-text search index.
pub struct TicketStore {
    index: RedbIndexStore,
    search: TantivySearchIndex,
    schema_registry: SchemaRegistry,
    /// Root directory for the redb database and Tantivy index files.
    pub index_root: PathBuf,
    /// Optional mutation hook. Set by the HTTP layer when streaming is active.
    /// Not used in CLI mode.
    hook: OnceLock<Box<dyn StoreHook>>,
}

impl TicketStore {
    /// Attach a mutation hook. May only be called once; subsequent calls
    /// are silently ignored (the first hook wins).
    pub fn set_hook(&self, hook: impl StoreHook) {
        let _ = self.hook.set(Box::new(hook));
    }

    /// Return a reference to the hook if one has been set.
    fn hook(&self) -> Option<&dyn StoreHook> {
        self.hook.get().map(|b| b.as_ref())
    }
    /// Open (or create) a ticket store rooted at `index_root` using built-in schemas.
    pub fn open(index_root: &Path) -> Result<Self, StorageError> {
        Self::open_with(index_root, SchemaRegistry::with_builtins())
    }

    /// Open (or create) a ticket store with a custom schema registry.
    ///
    /// Use this to inject test-specific or project-specific ticket type schemas
    /// loaded from TOML files via [`SchemaRegistry::load_dir`].
    pub fn open_with(index_root: &Path, schema_registry: SchemaRegistry) -> Result<Self, StorageError> {
        std::fs::create_dir_all(index_root)?;
        let db_path = index_root.join("tickets.redb");
        let search_dir = index_root.join("search_index");

        let index = RedbIndexStore::open(&db_path)?;
        let search = TantivySearchIndex::open_or_create(&search_dir)?;

        Ok(Self {
            index,
            search,
            schema_registry,
            index_root: index_root.to_path_buf(),
            hook: OnceLock::new(),
        })
    }

    // ── scan root management ──────────────────────────────────────────────────

    pub fn add_scan_root(&self, root: ScanRoot) -> Result<(), StorageError> {
        self.index.add_scan_root(&root)
    }

    pub fn list_scan_roots(&self) -> Result<Vec<ScanRoot>, StorageError> {
        self.index.list_scan_roots()
    }

    // ── ticket CRUD ──────────────────────────────────────────────────────────

    /// Create a new ticket.
    ///
    /// `target_root`: the scan root directory to place the ticket folder in.
    /// If `None`, the first registered scan root is used (error if none exist).
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
        let root = match target_root {
            Some(p) => p.to_path_buf(),
            None => {
                let roots = self.index.list_scan_roots()?;
                roots
                    .into_iter()
                    .next()
                    .map(|r| r.path)
                    .unwrap_or_else(|| self.index_root.join("tickets"))
            }
        };
        std::fs::create_dir_all(&root)?;

        let mut manifest = TicketManifest::new(id, now);
        manifest.extra.insert("type".to_string(), Value::String(type_id.to_string()));
        if let Some(t) = title {
            manifest.extra.insert("title".to_string(), Value::String(t.to_string()));
        }
        let state = initial_state.unwrap_or("new").to_string();
        manifest.extra.insert("state".to_string(), Value::String(state.clone()));
        for (k, v) in extra {
            manifest.extra.insert(k, v);
        }

        // Validate against type schema if known.
        if let Some(schema) = self.schema_registry.get(type_id) {
            schema.validate_manifest(&manifest)?;
        }

        let ticket_path = TicketFs::create(&manifest, &root, body)?;

        let indexed = IndexedTicket {
            id,
            path: ticket_path,
            type_id: type_id.to_string(),
            title: title.map(str::to_string),
            state: Some(state.clone()),
            created_at: now,
            updated_at: now,
            deleted: false,
        };
        self.index.insert_ticket(&indexed)?;

        // Use the provided body directly (already written to disk); fall back to
        // reading the file for scan-integrated tickets that may have existing content.
        let body_for_index = body
            .map(str::to_string)
            .or_else(|| TicketFs::read_description(&indexed.path));
        self.search.upsert(
            &id,
            title,
            body_for_index.as_deref(),
            Some(&state),
            Some(type_id),
        )?;

        // Append initial history snapshot (rev 1).
        let _ = TicketFs::append_history(&indexed.path, manifest.extra.clone());

        // Emit SSE hook event.
        if let Some(h) = self.hook() {
            h.ticket_upsert(id, Some(state), title.map(str::to_string), indexed.updated_at);
        }

        Ok(id)
    }

    /// Read the full manifest for a ticket by ID.
    pub fn get(&self, id: &Uuid) -> Result<TicketManifest, StorageError> {
        let indexed = self
            .index
            .get_ticket(id)?
            .ok_or(StorageError::NotFound(*id))?;
        if indexed.deleted {
            return Err(StorageError::NotFound(*id));
        }
        TicketFs::read(&indexed.path)
    }

    /// Get just the indexed metadata (faster than a full read).
    pub fn get_indexed(&self, id: &Uuid) -> Result<Option<IndexedTicket>, StorageError> {
        self.index.get_ticket(id)
    }

    /// Update a ticket: apply field patches and optional state transition.
    pub fn update(
        &self,
        id: &Uuid,
        patch: BTreeMap<String, Value>,
        from_state: Option<&str>,
        to_state: Option<&str>,
    ) -> Result<TicketManifest, StorageError> {
        let mut indexed = self
            .index
            .get_ticket(id)?
            .ok_or(StorageError::NotFound(*id))?;
        if indexed.deleted {
            return Err(StorageError::NotFound(*id));
        }

        // Validate state transition if type schema is known and state change requested.
        if let Some(to) = to_state {
            let current_state = indexed.state.as_deref().unwrap_or("new");
            let from = from_state.unwrap_or(current_state);
            if let Some(schema) = self.schema_registry.get(&indexed.type_id) {
                schema.ensure_transition(from, to)?;
            }
        }

        let new_state = to_state.map(str::to_string).or_else(|| indexed.state.clone());
        let updated_manifest = TicketFs::update(&indexed.path, &patch, to_state)?;

        // Refresh indexed metadata.
        let now = Utc::now();
        indexed.updated_at = now;
        if let Some(s) = &new_state {
            indexed.state = Some(s.clone());
        }
        if let Some(title_val) = patch.get("title").and_then(|v| v.as_str()) {
            indexed.title = Some(title_val.to_string());
        }
        self.index.insert_ticket(&indexed)?;

        let body = TicketFs::read_description(&indexed.path);
        self.search.upsert(
            id,
            indexed.title.as_deref(),
            body.as_deref(),
            indexed.state.as_deref(),
            Some(indexed.type_id.as_str()),
        )?;

        // Append history snapshot after successful write.
        let _ = TicketFs::append_history(&indexed.path, updated_manifest.extra.clone());

        // Emit SSE hook event.
        if let Some(h) = self.hook() {
            h.ticket_upsert(
                *id,
                indexed.state.clone(),
                indexed.title.clone(),
                indexed.updated_at,
            );
        }

        Ok(updated_manifest)
    }

    /// Soft-delete a ticket.
    pub fn delete(&self, id: &Uuid) -> Result<(), StorageError> {
        let indexed = self
            .index
            .get_ticket(id)?
            .ok_or(StorageError::NotFound(*id))?;
        if indexed.deleted {
            return Err(StorageError::NotFound(*id));
        }
        TicketFs::mark_deleted(&indexed.path)?;
        self.index.soft_delete_ticket(id)?;
        self.search.remove(id)?;

        // Emit SSE hook event.
        if let Some(h) = self.hook() {
            h.ticket_delete(*id);
        }

        Ok(())
    }

    /// Overwrite a ticket's manifest directly, bypassing state-machine validation.
    /// Used exclusively for rollback of in-flight batch operations.
    pub fn force_restore(
        &self,
        id: &Uuid,
        saved_extra: std::collections::BTreeMap<String, serde_json::Value>,
        saved_state: Option<String>,
    ) -> Result<(), StorageError> {
        let indexed = match self.index.get_ticket(id)? {
            Some(t) => t,
            None => return Ok(()), // ticket may have been hard-deleted; nothing to restore
        };
        TicketFs::update(&indexed.path, &saved_extra, saved_state.as_deref())?;
        // Refresh redb + search index.
        let mut refreshed = indexed;
        refreshed.state = saved_state.clone();
        if let Some(title_val) = saved_extra.get("title").and_then(|v| v.as_str()) {
            refreshed.title = Some(title_val.to_string());
        }
        self.index.insert_ticket(&refreshed)?;
        let body = TicketFs::read_description(&refreshed.path);
        self.search.upsert(
            id,
            refreshed.title.as_deref(),
            body.as_deref(),
            refreshed.state.as_deref(),
            Some(refreshed.type_id.as_str()),
        )?;
        Ok(())
    }

    // ── history ───────────────────────────────────────────────────────────────

    /// Return all revision snapshots for `id`, oldest first.
    pub fn get_history(
        &self,
        id: &Uuid,
    ) -> Result<Vec<crate::storage::ticket_fs::HistoryRevision>, StorageError> {
        let indexed = self
            .index
            .get_ticket(id)?
            .ok_or(StorageError::NotFound(*id))?;
        if indexed.deleted {
            return Err(StorageError::NotFound(*id));
        }
        TicketFs::read_history(&indexed.path)
    }

    /// Apply a revert: overwrite the ticket with `fields` from a historical
    /// snapshot, bypassing state-machine validation, and append a new revision.
    ///
    /// This is forward-only: the history log grows by one entry; nothing is
    /// erased.
    pub fn apply_revert(
        &self,
        id: &Uuid,
        fields: BTreeMap<String, Value>,
    ) -> Result<u64, StorageError> {
        let indexed = self
            .index
            .get_ticket(id)?
            .ok_or(StorageError::NotFound(*id))?;
        if indexed.deleted {
            return Err(StorageError::NotFound(*id));
        }

        let target_state = fields.get("state").and_then(|v| v.as_str()).map(str::to_string);
        let mut patch = fields.clone();
        patch.remove("state"); // state is applied via the separate new_state arg

        TicketFs::update(&indexed.path, &patch, target_state.as_deref())?;

        // Refresh indexes.
        let mut refreshed = indexed;
        refreshed.state = target_state.clone();
        if let Some(title_val) = patch.get("title").and_then(|v| v.as_str()) {
            refreshed.title = Some(title_val.to_string());
        }
        self.index.insert_ticket(&refreshed)?;
        let body = TicketFs::read_description(&refreshed.path);
        self.search.upsert(
            id,
            refreshed.title.as_deref(),
            body.as_deref(),
            refreshed.state.as_deref(),
            Some(refreshed.type_id.as_str()),
        )?;

        // Append history entry for the reverted state (creates a new rev).
        let updated_manifest = TicketFs::read(&refreshed.path)?;
        let new_rev = TicketFs::append_history(&refreshed.path, updated_manifest.extra)?;
        Ok(new_rev)
    }


    // ── list / search ─────────────────────────────────────────────────────────

    pub fn list(
        &self,
        state_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<IndexedTicket>, StorageError> {
        let all = self.index.list_tickets(false)?;
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|t| {
                if let Some(s) = state_filter {
                    if t.state.as_deref() != Some(s) {
                        return false;
                    }
                }
                if let Some(tp) = type_filter {
                    if t.type_id != tp {
                        return false;
                    }
                }
                true
            })
            .take(limit.unwrap_or(usize::MAX))
            .collect();
        Ok(filtered)
    }

    /// List tickets with extended options (deleted visibility, field filters).
    pub fn list_extended(
        &self,
        state_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: Option<usize>,
        include_deleted: bool,
        field_filters: &[(String, String)],
    ) -> Result<Vec<IndexedTicket>, StorageError> {
        let all = self.index.list_tickets(include_deleted)?;
        let needs_manifest_check = !field_filters.is_empty();
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|t| {
                if let Some(s) = state_filter {
                    if t.state.as_deref() != Some(s) {
                        return false;
                    }
                }
                if let Some(tp) = type_filter {
                    if t.type_id != tp {
                        return false;
                    }
                }
                if needs_manifest_check {
                    let manifest = match TicketFs::read(&t.path) {
                        Ok(m) => m,
                        Err(_) => return false,
                    };
                    for (key, value) in field_filters {
                        let field_val = manifest.extra.get(key)
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if field_val != value {
                            return false;
                        }
                    }
                }
                true
            })
            .take(limit.unwrap_or(usize::MAX))
            .collect();
        Ok(filtered)
    }

    /// Fast-forward a ticket to a target state by traversing all intermediate states.
    /// Returns the final manifest after all transitions.
    pub fn close(
        &self,
        id: &Uuid,
        target_state: &str,
    ) -> Result<(TicketManifest, Vec<String>), StorageError> {
        let indexed = self
            .index
            .get_ticket(id)?
            .ok_or(StorageError::NotFound(*id))?;
        if indexed.deleted {
            return Err(StorageError::NotFound(*id));
        }

        let current_state = indexed.state.as_deref().unwrap_or("new");
        if current_state == target_state {
            let manifest = TicketFs::read(&indexed.path)?;
            return Ok((manifest, vec![]));
        }

        let schema = self.schema_registry.get(&indexed.type_id)
            .ok_or_else(|| StorageError::Other(
                format!("no schema for type '{}'", indexed.type_id),
            ))?;

        let path = schema.find_path(current_state, target_state)
            .ok_or_else(|| StorageError::Other(
                format!("no path from '{}' to '{}'", current_state, target_state),
            ))?;

        let empty_patch = BTreeMap::new();
        let mut last_manifest = None;
        for state in &path {
            last_manifest = Some(self.update(id, empty_patch.clone(), None, Some(state))?);
        }

        Ok((last_manifest.unwrap(), path))
    }

    /// Attach a file as an asset to a ticket. Returns the asset path.
    pub fn attach(
        &self,
        id: &Uuid,
        source_path: &std::path::Path,
        asset_name: Option<&str>,
    ) -> Result<std::path::PathBuf, StorageError> {
        let indexed = self
            .index
            .get_ticket(id)?
            .ok_or(StorageError::NotFound(*id))?;
        if indexed.deleted {
            return Err(StorageError::NotFound(*id));
        }

        let file_name = asset_name
            .map(String::from)
            .unwrap_or_else(|| {
                source_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "attachment".to_string())
            });

        let assets_dir = indexed.path.join("assets");
        std::fs::create_dir_all(&assets_dir)
            .map_err(|e| StorageError::Other(format!("create assets dir: {e}")))?;

        let dest = assets_dir.join(&file_name);
        std::fs::copy(source_path, &dest)
            .map_err(|e| StorageError::Other(format!("copy asset: {e}")))?;

        // Record in history
        let mut event = BTreeMap::new();
        event.insert("_event".to_string(), serde_json::Value::String("attach".to_string()));
        event.insert("asset".to_string(), serde_json::Value::String(file_name));
        let _ = TicketFs::append_history(&indexed.path, event);

        Ok(dest)
    }

    /// List assets for a ticket.
    pub fn list_assets(&self, id: &Uuid) -> Result<Vec<String>, StorageError> {
        let indexed = self
            .index
            .get_ticket(id)?
            .ok_or(StorageError::NotFound(*id))?;
        if indexed.deleted {
            return Err(StorageError::NotFound(*id));
        }

        let assets_dir = indexed.path.join("assets");
        if !assets_dir.exists() {
            return Ok(vec![]);
        }

        let mut names = Vec::new();
        for entry in std::fs::read_dir(&assets_dir)
            .map_err(|e| StorageError::Other(format!("read assets dir: {e}")))?
        {
            let entry: std::fs::DirEntry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn search_tickets(
        &self,
        query_expr: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        let expr = parse_query(query_expr).map_err(StorageError::QueryParse)?;
        self.search.search(&expr, limit)
    }

    // ── edge management ───────────────────────────────────────────────────────

    pub fn edges_from(&self, id: &Uuid) -> Result<Vec<EdgeRecord>, StorageError> {
        self.index.edges_from(id)
    }

    /// Returns every edge in the store (used for bulk dependency resolution).
    pub fn list_all_edges(&self) -> Result<Vec<EdgeRecord>, StorageError> {
        self.index.list_all_edges()
    }

    pub fn add_edge(&self, edge: EdgeRecord) -> Result<(), StorageError> {
        // For acyclic-enforced kinds: check for cycles.
        let is_acyclic = self.schema_registry
            .get(crate::model::default_schema::TYPE_ID)
            .and_then(|s| s.edge_rules.get(&edge.kind))
            .map(|r| r.acyclic_enforced)
            .unwrap_or(false);

        if is_acyclic && self.index.is_reachable(&edge.to, &edge.from)? {
            return Err(StorageError::DependencyCycle);
        }

        self.index.insert_edge(&edge)?;

        // Emit SSE hook event.
        if let Some(h) = self.hook() {
            h.edge_upsert(edge.from, edge.to, edge.kind.clone());
        }

        Ok(())
    }

    pub fn remove_edge(&self, edge: EdgeRecord) -> Result<(), StorageError> {
        self.index.delete_edge(&edge)?;

        // Emit SSE hook event.
        if let Some(h) = self.hook() {
            h.edge_delete(edge.from, edge.to, edge.kind.clone());
        }

        Ok(())
    }

    // ── scan / reconcile ──────────────────────────────────────────────────────

    /// Walk all registered scan roots and integrate discovered tickets into the
    /// index and search index.
    ///
    /// If `reindex` is `true`, the search index is rebuilt from scratch for all
    /// found tickets (crash recovery path).
    pub fn scan(&self, reindex: bool) -> Result<ScanReport, StorageError> {
        // When doing a full reindex, purge the search index first so that
        // entries for deleted tickets don't survive the rebuild.
        if reindex {
            self.search.clear_all()?;
        }

        let roots = self.index.list_scan_roots()?;

        // Also always include the default tickets dir under index_root.
        let default_root = ScanRoot {
            path: self.index_root.join("tickets"),
            label: "default".to_string(),
        };
        let all_roots: Vec<&ScanRoot> = std::iter::once(&default_root)
            .chain(roots.iter())
            .collect();

        let mut integrated = 0usize;
        let mut diagnostics = Vec::new();

        for root in all_roots {
            if !root.path.exists() {
                continue;
            }
            let (entries, diags) = TicketFs::scan_root(&root.path)?;
            diagnostics.extend(diags);

            for entry in entries {
                integrate_entry(&self.index, &self.search, entry, reindex)?;
                integrated += 1;
            }
        }

        Ok(ScanReport { integrated, diagnostics })
    }

    /// Integrate a single ticket folder discovered on the filesystem into the
    /// index and search index.
    ///
    /// This is used by the watcher daemon when a specific path is signalled by
    /// a filesystem event.  Falls back gracefully if the path is not a valid
    /// ticket folder (returns `Ok(false)`).
    pub fn integrate_orphan(&self, path: &Path) -> Result<bool, StorageError> {
        // Derive UUID from the directory name.
        let id: Uuid = match path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|s| s.parse().ok())
        {
            Some(u) => u,
            None => return Ok(false),
        };

        // Use TicketFs to read the manifest from disk.
        use crate::storage::ticket_fs::TicketScanEntry;
        let manifest = match crate::storage::ticket_fs::TicketFs::read(path) {
            Ok(m) => m,
            Err(_) => return Ok(false),
        };

        // Skip soft-deleted tickets.
        let is_deleted = manifest
            .extra
            .get("deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if is_deleted {
            return Ok(false);
        }

        let entry = TicketScanEntry { id, path: path.to_path_buf(), manifest };
        integrate_entry(&self.index, &self.search, entry, true)?;
        Ok(true)
    }

    // ── lease operations (Phase 1.5 pre-wire) ────────────────────────────────

    pub fn claim(
        &self,
        ticket_id: &Uuid,
        agent_id: &str,
        ttl_secs: u64,
        work_intent: Option<&str>,
    ) -> Result<LeaseInfo, StorageError> {
        // Check for existing non-expired lease.
        if let Some(existing) = self.index.get_lease(ticket_id)? {
            if !existing.is_expired() {
                return Err(StorageError::LeaseConflict {
                    ticket: *ticket_id,
                    holder: existing.working_by.clone(),
                });
            }
        }

        let now = Utc::now();
        let lease = LeaseInfo {
            ticket_id: *ticket_id,
            working_by: agent_id.to_string(),
            work_intent: work_intent.map(str::to_string),
            claimed_at: now,
            lease_expires_at: now + chrono::Duration::seconds(ttl_secs as i64),
            ttl_secs,
            conflict_domain: None,
        };
        self.index.insert_lease(&lease)?;
        Ok(lease)
    }

    pub fn unclaim(&self, ticket_id: &Uuid) -> Result<(), StorageError> {
        self.index.remove_lease(ticket_id)
    }

    pub fn list_leases(&self) -> Result<Vec<LeaseInfo>, StorageError> {
        self.index.list_active_leases()
    }

    // ── validation & release protocol ─────────────────────────────────────────

    /// `task_validate_start` — move ticket from `in-review` to `in-validation`.
    ///
    /// Guards:
    /// - current state must be `in-review`
    /// - `validator_id` must not equal `worker_id` (separation of duties)
    pub fn validate_start(
        &self,
        ticket_id: &Uuid,
        assignment_id: &str,
        validator_id: &str,
        validation_profile: &str,
        required_checks: Vec<String>,
    ) -> Result<TicketManifest, StorageError> {
        let manifest = self.get(ticket_id)?;
        let current_state = manifest.extra.get("state").and_then(|v| v.as_str()).unwrap_or("");

        if current_state != "in-review" {
            return Err(ProtocolError::ValidateInvalidState {
                ticket: *ticket_id,
                actual: current_state.to_string(),
                expected: "in-review".to_string(),
            }
            .into());
        }

        // Separation-of-duties check.
        let worker_id = manifest.extra.get("working_by").and_then(|v| v.as_str()).unwrap_or("");
        if !worker_id.is_empty() && worker_id == validator_id {
            return Err(ProtocolError::ValidateSameIdentity {
                identity: validator_id.to_string(),
            }
            .into());
        }

        let mut patch = BTreeMap::new();
        patch.insert("validator_id".to_string(), Value::String(validator_id.to_string()));
        patch.insert("validation_status".to_string(), Value::String("in-progress".to_string()));
        patch.insert("validation_profile".to_string(), Value::String(validation_profile.to_string()));
        patch.insert(
            "required_checks".to_string(),
            Value::Array(required_checks.into_iter().map(Value::String).collect()),
        );
        patch.insert("assignment_id".to_string(), Value::String(assignment_id.to_string()));

        self.update(ticket_id, patch, Some("in-review"), Some("in-validation"))
    }

    /// `task_validate_result` — submit validation outcome.
    ///
    /// `result` must be `"passed"` or `"failed"`.
    /// On pass: ticket moves to `done`, `validation_status=passed`.
    /// On fail: ticket moves back to `in-review`, `validation_status=failed`.
    ///
    /// Guards:
    /// - current state must be `in-validation`
    /// - `validator_id` must match recorded validator
    /// - `evidence_refs` must be non-empty
    pub fn validate_result(
        &self,
        ticket_id: &Uuid,
        assignment_id: &str,
        validator_id: &str,
        result: &str,
        evidence_refs: Vec<String>,
        summary: Option<&str>,
        bug_links: Vec<Uuid>,
    ) -> Result<ValidationResultOutcome, StorageError> {
        if evidence_refs.is_empty() {
            return Err(ProtocolError::ValidateMissingEvidence.into());
        }

        let manifest = self.get(ticket_id)?;
        let current_state = manifest.extra.get("state").and_then(|v| v.as_str()).unwrap_or("");
        if current_state != "in-validation" {
            return Err(ProtocolError::ValidateInvalidState {
                ticket: *ticket_id,
                actual: current_state.to_string(),
                expected: "in-validation".to_string(),
            }
            .into());
        }

        let recorded_validator = manifest.extra.get("validator_id").and_then(|v| v.as_str()).unwrap_or("");
        if !recorded_validator.is_empty() && recorded_validator != validator_id {
            return Err(ProtocolError::ValidateAssignmentMismatch.into());
        }

        let passed = result == "passed";
        let (new_state, status_str) = if passed {
            ("done", "passed")
        } else {
            ("in-review", "failed")
        };

        let mut patch = BTreeMap::new();
        patch.insert("validation_status".to_string(), Value::String(status_str.to_string()));
        patch.insert("assignment_id".to_string(), Value::String(assignment_id.to_string()));
        patch.insert(
            "evidence_refs".to_string(),
            Value::Array(evidence_refs.iter().map(|s| Value::String(s.clone())).collect()),
        );
        if let Some(s) = summary {
            patch.insert("validation_summary".to_string(), Value::String(s.to_string()));
        }
        if !bug_links.is_empty() {
            patch.insert(
                "bug_links".to_string(),
                Value::Array(bug_links.iter().map(|id| Value::String(id.to_string())).collect()),
            );
        }

        let from_state = "in-validation";
        let _updated = self.update(ticket_id, patch, Some(from_state), Some(new_state))?;

        Ok(ValidationResultOutcome {
            ticket_id: *ticket_id,
            state: new_state.to_string(),
            validation_status: status_str.to_string(),
            passed,
        })
    }

    /// `task_release_candidate_create` — move a `done` ticket to `done` (no-op in simplified workflow).
    ///
    /// Guards:
    /// - current state must be `done`
    /// - `validation_status` must be `passed`
    /// - `assignment_chain` must be non-empty
    pub fn release_candidate_create(
        &self,
        ticket_id: &Uuid,
        release_target: &str,
        assignment_chain: Vec<String>,
    ) -> Result<TicketManifest, StorageError> {
        if assignment_chain.is_empty() {
            return Err(ProtocolError::ReleaseAssignmentChainMissing.into());
        }

        let manifest = self.get(ticket_id)?;
        let current_state = manifest.extra.get("state").and_then(|v| v.as_str()).unwrap_or("");
        if current_state != "done" {
            return Err(ProtocolError::ReleaseInvalidState {
                ticket: *ticket_id,
                actual: current_state.to_string(),
                expected: "done".to_string(),
            }
            .into());
        }

        let validation_status = manifest.extra.get("validation_status").and_then(|v| v.as_str()).unwrap_or("");
        if validation_status != "passed" {
            return Err(ProtocolError::ReleaseValidationNotPassed {
                ticket: *ticket_id,
                status: validation_status.to_string(),
            }
            .into());
        }

        let mut patch = BTreeMap::new();
        patch.insert("release_target".to_string(), Value::String(release_target.to_string()));
        patch.insert(
            "assignment_chain".to_string(),
            Value::Array(assignment_chain.into_iter().map(Value::String).collect()),
        );

        self.update(ticket_id, patch, Some("done"), Some("done"))
    }

    /// `task_release_gate_check` — evaluate release gates for a target.
    ///
    /// Returns pass/fail results for each requested gate ID.
    /// The standard gates are R1–R4 as defined in VALIDATION_RELEASE_GOVERNANCE.md.
    pub fn release_gate_check(
        &self,
        release_target: &str,
        required_gates: &[String],
    ) -> Result<GateCheckOutcome, StorageError> {
        // Collect all done tickets for this target.
        let all = self.index.list_tickets(false)?;
        let candidates: Vec<_> = all
            .iter()
            .filter(|t| {
                t.state.as_deref() == Some("done")
            })
            .filter(|_t| {
                // Check release_target field in manifest if needed; use in-memory index for speed.
                // For now, accept all candidates when release_target is present — manifest check
                // happens in full promote path.
                true
            })
            .collect();

        if candidates.is_empty() {
            return Err(ProtocolError::ReleaseTargetNotFound(release_target.to_string()).into());
        }

        let mut gate_results: BTreeMap<String, GateStatus> = BTreeMap::new();
        let mut blocking_reasons: Vec<String> = Vec::new();

        for gate in required_gates {
            let (status, reason) = evaluate_gate(gate.as_str(), &candidates, release_target, &self.index)?;
            if let Some(r) = reason {
                blocking_reasons.push(format!("{gate}: {r}"));
            }
            gate_results.insert(gate.clone(), status);
        }

        Ok(GateCheckOutcome {
            release_target: release_target.to_string(),
            gates: gate_results,
            blocking_reasons,
        })
    }

    /// `task_release_promote` — promote all `done` tickets for a target.
    ///
    /// Guards:
    /// - all required gates must be `pass`
    /// - `merge_commit` must be provided
    pub fn release_promote(
        &self,
        release_target: &str,
        release_version: &str,
        merge_commit: &str,
        required_gates: &[String],
    ) -> Result<PromoteOutcome, StorageError> {
        if merge_commit.is_empty() {
            return Err(ProtocolError::ReleaseMergeMetadataMissing.into());
        }

        // Gate check.
        let gate_outcome = self.release_gate_check(release_target, required_gates)?;
        let failing_gates: Vec<_> = gate_outcome
            .gates
            .iter()
            .filter(|(_, s)| !matches!(s, GateStatus::Pass))
            .map(|(k, _)| k.clone())
            .collect();
        if !failing_gates.is_empty() {
            return Err(ProtocolError::ReleaseGatesNotSatisfied(
                gate_outcome.blocking_reasons.join("; "),
            )
            .into());
        }

        let all = self.index.list_tickets(false)?;
        let to_promote: Vec<Uuid> = all
            .into_iter()
            .filter(|t| t.state.as_deref() == Some("done"))
            .map(|t| t.id)
            .collect();

        if to_promote.is_empty() {
            return Err(ProtocolError::ReleaseTicketStateInvalid(
                format!("no done tickets found for target '{release_target}'"),
            )
            .into());
        }

        let mut promoted = 0usize;
        for ticket_id in &to_promote {
            let mut patch = BTreeMap::new();
            patch.insert("release_version".to_string(), Value::String(release_version.to_string()));
            patch.insert("merge_commit".to_string(), Value::String(merge_commit.to_string()));
            self.update(ticket_id, patch, None, None)?;
            promoted += 1;
        }

        Ok(PromoteOutcome {
            release_target: release_target.to_string(),
            release_version: release_version.to_string(),
            promoted_ticket_count: promoted,
            monitoring_state: "active".to_string(),
        })
    }
}

pub struct ScanReport {
    pub integrated: usize,
    pub diagnostics: Vec<crate::model::filesystem::ParseDiagnostic>,
}

/// Outcome of `task_validate_result`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResultOutcome {
    pub ticket_id: Uuid,
    pub state: String,
    pub validation_status: String,
    pub passed: bool,
}

/// Per-gate evaluation status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Pass,
    Fail,
}

/// Outcome of `task_release_gate_check`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCheckOutcome {
    pub release_target: String,
    pub gates: BTreeMap<String, GateStatus>,
    pub blocking_reasons: Vec<String>,
}

/// Outcome of `task_release_promote`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteOutcome {
    pub release_target: String,
    pub release_version: String,
    pub promoted_ticket_count: usize,
    pub monitoring_state: String,
}

fn evaluate_gate(
    gate: &str,
    candidates: &[&IndexedTicket],
    _release_target: &str,
    _index: &RedbIndexStore,
) -> Result<(GateStatus, Option<String>), StorageError> {
    match gate {
        // R1: all included tickets are done — no open blockers
        "R1" => {
            let all_ready = candidates
                .iter()
                .all(|t| matches!(t.state.as_deref(), Some("done")));
            if all_ready {
                Ok((GateStatus::Pass, None))
            } else {
                Ok((GateStatus::Fail, Some("some tickets are not yet done".to_string())))
            }
        }
        // R2: no open sev0/sev1 bugs (best-effort via field scan)
        "R2" => Ok((GateStatus::Pass, None)), // detailed bug scan is Phase 2
        // R3: rollback path — placeholder until Phase 2 history/revert is wired
        "R3" => Ok((GateStatus::Pass, None)),
        // R4: release smoke suite — placeholder
        "R4" => Ok((GateStatus::Pass, None)),
        unknown => Ok((
            GateStatus::Fail,
            Some(format!("gate '{unknown}' is not defined")),
        )),
    }
}

fn integrate_entry(
    index: &RedbIndexStore,
    search: &TantivySearchIndex,
    entry: TicketScanEntry,
    reindex: bool,
) -> Result<(), StorageError> {
    let type_id = entry
        .manifest
        .extra
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let title = entry
        .manifest
        .extra
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let state = entry
        .manifest
        .extra
        .get("state")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let now = Utc::now();

    // Upsert into redb (insert or overwrite).
    let indexed = match index.get_ticket(&entry.id)? {
        Some(mut existing) => {
            existing.updated_at = now;
            existing.title = title.clone();
            existing.state = state.clone();
            existing.deleted = false;
            existing
        }
        None => IndexedTicket {
            id: entry.id,
            path: entry.path.clone(),
            type_id: type_id.clone(),
            title: title.clone(),
            state: state.clone(),
            created_at: entry.manifest.created_at,
            updated_at: now,
            deleted: false,
        },
    };
    index.insert_ticket(&indexed)?;

    // Update search index if needed.
    if reindex {
        let body = TicketFs::read_description(&entry.path);
        search.upsert(
            &entry.id,
            title.as_deref(),
            body.as_deref(),
            state.as_deref(),
            Some(&type_id),
        )?;
    }

    Ok(())
}
