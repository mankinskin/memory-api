use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::error::StorageError;
use crate::model::schema_registry::SchemaRegistry;
use crate::model::ticket::{TicketId, TicketManifest};
use crate::storage::index::RedbIndexStore;
use crate::storage::indexed::IndexedTicket;
use crate::storage::search::TantivySearchIndex;
use crate::storage::ticket_fs::TicketFs;

mod board;
mod lifecycle;
mod query;
mod release;
mod scan;

pub use self::release::{GateCheckOutcome, GateStatus, PromoteOutcome, ValidationResultOutcome};
pub use self::scan::ScanReport;

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

/// The central ticket store: filesystem source-of-truth + SQLite metadata index +
/// Tantivy full-text search index.
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
        let db_path = index_root.join("tickets.db");
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

    /// Access the schema registry to look up type schemas.
    pub fn schema_registry(&self) -> &SchemaRegistry {
        &self.schema_registry
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
        let _ = TicketFs::append_history(&indexed.path, manifest.extra.clone(), None);

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

    /// Fetch multiple tickets by ID in a single ReDB read transaction.
    ///
    /// Returns a `HashMap<Uuid, IndexedTicket>` for O(1) lookup. Missing or
    /// deleted IDs are omitted. Prefer this over N separate `get_indexed()`
    /// calls when you need metadata for a known set of IDs (e.g. BFS nodes).
    pub fn get_indexed_many(&self, ids: &[Uuid]) -> Result<std::collections::HashMap<Uuid, IndexedTicket>, StorageError> {
        self.index.get_tickets_by_ids(ids)
    }

    /// Update a ticket: apply field patches, optional state transition, and optional description.
    pub fn update(
        &self,
        id: &Uuid,
        patch: BTreeMap<String, Value>,
        from_state: Option<&str>,
        to_state: Option<&str>,
        description: Option<&str>,
        author: Option<&str>,
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
                // Enforce required_states before entering a terminal state.
                if !schema.required_states.is_empty()
                    && schema.terminal_states.contains(&to.to_string())
                {
                    let history = TicketFs::read_history(&indexed.path).unwrap_or_default();
                    let visited: Vec<String> = history
                        .iter()
                        .filter_map(|r| r.fields.get("state").and_then(|v| v.as_str()).map(String::from))
                        .collect();
                    schema.validate_workflow(to, &visited)?;
                }
            }
        }

        let new_state = to_state.map(str::to_string).or_else(|| indexed.state.clone());
        let updated_manifest = TicketFs::update(&indexed.path, &patch, to_state)?;

        // Write description if provided.
        if let Some(desc) = description {
            TicketFs::write_description(&indexed.path, desc)?;
        }

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
        let _ = TicketFs::append_history(&indexed.path, updated_manifest.extra.clone(), author.map(str::to_string));

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

        Ok(updated_manifest)
    }

}
