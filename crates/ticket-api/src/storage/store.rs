use std::{
    collections::{
        BTreeMap,
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

use chrono::Utc;
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

    fn normalize_path(path: PathBuf) -> PathBuf {
        #[cfg(windows)]
        {
            let raw = path.to_string_lossy().replace('\\', "/");
            let normalized = raw
                .strip_prefix("//?/")
                .or_else(|| raw.strip_prefix(r"\\?\"))
                .unwrap_or(&raw);
            PathBuf::from(normalized)
        }

        #[cfg(not(windows))]
        {
            path
        }
    }

    fn normalize_existing_path(path: &Path) -> PathBuf {
        std::fs::canonicalize(path)
            .map(Self::normalize_path)
            .unwrap_or_else(|_| Self::normalize_path(path.to_path_buf()))
    }

    fn resolved_candidate_matches(
        candidate: &Path,
        marker_file: Option<&str>,
    ) -> bool {
        match marker_file {
            Some(marker_file) => candidate.join(marker_file).is_file(),
            None => candidate.is_dir(),
        }
    }

    fn resolve_indexed_path(
        &self,
        path: &Path,
        marker_file: Option<&str>,
    ) -> PathBuf {
        if path.is_absolute() && Self::resolved_candidate_matches(path, marker_file) {
            return Self::normalize_path(path.to_path_buf());
        }

        if Self::resolved_candidate_matches(path, marker_file) {
            return Self::normalize_existing_path(path);
        }

        for base in self.index_root.ancestors() {
            let candidate = base.join(path);
            if Self::resolved_candidate_matches(&candidate, marker_file) {
                return Self::normalize_existing_path(&candidate);
            }
        }

        Self::normalize_path(path.to_path_buf())
    }

    pub(super) fn resolve_ticket_path(
        &self,
        path: &Path,
    ) -> PathBuf {
        self.resolve_indexed_path(path, Some(TICKET_MANIFEST_FILE))
    }

    pub(super) fn resolve_scan_root_path(
        &self,
        path: &Path,
    ) -> PathBuf {
        self.resolve_indexed_path(path, None)
    }

    pub(super) fn normalize_indexed_ticket(
        &self,
        mut indexed: IndexedTicket,
    ) -> IndexedTicket {
        indexed.path = self.resolve_ticket_path(&indexed.path);
        indexed
    }

    pub(super) fn normalize_indexed_tickets(
        &self,
        tickets: Vec<IndexedTicket>,
    ) -> Vec<IndexedTicket> {
        tickets
            .into_iter()
            .map(|ticket| self.normalize_indexed_ticket(ticket))
            .collect()
    }

    /// Open an existing ticket store rooted at `index_root` using built-in schemas.
    ///
    /// Returns [`StorageError::WorkspaceNotFound`] if the workspace has not been
    /// initialized yet. Run `ticket init` to create a new workspace first.
    pub fn open(index_root: &Path) -> Result<Self, StorageError> {
        Self::open_with(index_root, SchemaRegistry::with_builtins())
    }

    /// Open an existing ticket store with a custom schema registry.
    ///
    /// Returns [`StorageError::WorkspaceNotFound`] if the workspace has not been
    /// initialized yet. Use [`TicketStore::init_with`] to create a new workspace.
    pub fn open_with(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<Self, StorageError> {
        let (store, _) = Self::open_with_profiled(index_root, schema_registry)?;
        Ok(store)
    }

    pub fn open_with_profiled(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<(Self, StoreOpenReport), StorageError> {
        let _span_guard = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_open_profiled"
        )
        .entered();
        let overall_started = Instant::now();
        let resolve_started = Instant::now();
        let index_root = workspace::resolve_store_root_from(
            index_root,
            workspace::TICKET_INDEX_DIR,
        );
        let mut report = StoreOpenReport::default();
        report.phase_timings_ms.insert(
            "resolve_store_root_ms".to_string(),
            elapsed_ms(resolve_started),
        );
        let existence_started = Instant::now();
        if !index_root.join("tickets.db").is_file() {
            return Err(StorageError::WorkspaceNotFound { path: index_root });
        }
        report.phase_timings_ms.insert(
            "workspace_exists_check_ms".to_string(),
            elapsed_ms(existence_started),
        );
        let (store, internal_report) =
            Self::open_internal_profiled(index_root, schema_registry)?;
        merge_timings(&mut report.phase_timings_ms, internal_report.phase_timings_ms);
        report.scan_reports.extend(internal_report.scan_reports);
        report.phase_timings_ms.insert(
            "open_total_ms".to_string(),
            elapsed_ms(overall_started),
        );
        emit_store_open_report("ticket_store_open_profiled_complete", &report);
        Ok((store, report))
    }

    /// Initialize a new ticket store rooted at `index_root` using built-in schemas.
    ///
    /// Creates the workspace directory and all required index files. Idempotent:
    /// if the workspace already exists it is opened without error.
    pub fn init(index_root: &Path) -> Result<Self, StorageError> {
        Self::init_with(index_root, SchemaRegistry::with_builtins())
    }

    /// Open an existing ticket store, or initialize and rebuild it when the
    /// local derived index artifacts do not exist yet.
    pub fn open_or_init(index_root: &Path) -> Result<Self, StorageError> {
        Self::open_or_init_with(index_root, SchemaRegistry::with_builtins())
    }

    /// Initialize a new ticket store with a custom schema registry.
    ///
    /// Creates the workspace directory and all required index files. Idempotent:
    /// if the workspace already exists it is opened without error.
    pub fn init_with(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<Self, StorageError> {
        let (store, _) = Self::init_with_profiled(index_root, schema_registry)?;
        Ok(store)
    }

    fn init_with_profiled(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<(Self, StoreOpenReport), StorageError> {
        let _span_guard = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_init_profiled"
        )
        .entered();
        let overall_started = Instant::now();
        let resolve_started = Instant::now();
        let index_root = workspace::resolve_store_root_from(
            index_root,
            workspace::TICKET_INDEX_DIR,
        );
        let mut report = StoreOpenReport {
            initialized_store: true,
            ..Default::default()
        };
        report.phase_timings_ms.insert(
            "resolve_store_root_ms".to_string(),
            elapsed_ms(resolve_started),
        );
        let ensure_started = Instant::now();
        ensure_sqlite_index_root(
            &index_root,
            "tickets.db",
            &["search_index/"],
        )?;
        report.phase_timings_ms.insert(
            "ensure_index_root_ms".to_string(),
            elapsed_ms(ensure_started),
        );
        let (store, internal_report) =
            Self::open_internal_profiled(index_root, schema_registry)?;
        merge_timings(&mut report.phase_timings_ms, internal_report.phase_timings_ms);
        report.scan_reports.extend(internal_report.scan_reports);
        report.phase_timings_ms.insert(
            "init_total_ms".to_string(),
            elapsed_ms(overall_started),
        );
        emit_store_open_report("ticket_store_init_profiled_complete", &report);
        Ok((store, report))
    }

    /// Open an existing ticket store with a custom schema registry, or
    /// initialize and rebuild it when the local derived index artifacts do not
    /// exist yet.
    pub fn open_or_init_with(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<Self, StorageError> {
        let (store, _) = Self::open_or_init_with_profiled(index_root, schema_registry)?;
        Ok(store)
    }

    pub fn open_or_init_profiled(
        index_root: &Path,
    ) -> Result<(Self, StoreOpenReport), StorageError> {
        Self::open_or_init_with_profiled(
            index_root,
            SchemaRegistry::with_builtins(),
        )
    }

    pub fn open_or_init_with_profiled(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<(Self, StoreOpenReport), StorageError> {
        let span = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_open_or_init",
            initialized_store = Empty,
        );
        let _span_guard = span.enter();
        let overall_started = Instant::now();
        match Self::open_with_profiled(index_root, schema_registry.clone()) {
            Ok((store, mut report)) => {
                span.record("initialized_store", false);
                report.phase_timings_ms.insert(
                    "open_or_init_total_ms".to_string(),
                    elapsed_ms(overall_started),
                );
                emit_store_open_report(
                    "ticket_store_open_or_init_complete",
                    &report,
                );
                Ok((store, report))
            }
            Err(StorageError::WorkspaceNotFound { .. }) => {
                span.record("initialized_store", true);
                let (store, mut report) =
                    Self::init_with_profiled(index_root, schema_registry)?;
                let initial_scan_started = Instant::now();
                let initial_scan = store.scan(true)?;
                report.phase_timings_ms.insert(
                    "post_init_scan_ms".to_string(),
                    elapsed_ms(initial_scan_started),
                );
                report
                    .scan_reports
                    .insert("post_init_scan".to_string(), initial_scan);
                report.phase_timings_ms.insert(
                    "open_or_init_total_ms".to_string(),
                    elapsed_ms(overall_started),
                );
                emit_store_open_report(
                    "ticket_store_open_or_init_complete",
                    &report,
                );
                Ok((store, report))
            }
            Err(error) => Err(error),
        }
    }

    fn open_internal_profiled(
        index_root: std::path::PathBuf,
        schema_registry: SchemaRegistry,
    ) -> Result<(Self, StoreOpenReport), StorageError> {
        let _span_guard = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_open_internal"
        )
        .entered();
        let overall_started = Instant::now();
        let normalize_started = Instant::now();
        let index_root = Self::normalize_existing_path(&index_root);
        let mut report = StoreOpenReport::default();
        report.phase_timings_ms.insert(
            "normalize_existing_path_ms".to_string(),
            elapsed_ms(normalize_started),
        );
        let db_path = index_root.join("tickets.db");
        let search_dir = index_root.join("search_index");

        let sqlite_started = Instant::now();
        let index = RedbIndexStore::open(&db_path)?;
        report.phase_timings_ms.insert(
            "open_sqlite_index_ms".to_string(),
            elapsed_ms(sqlite_started),
        );
        let search_started = Instant::now();
        let search = match TantivySearchIndex::open_or_create(&search_dir) {
            Ok(search) => search,
            Err(_) => {
                let _ = std::fs::remove_dir_all(&search_dir);
                TantivySearchIndex::open_or_create(&search_dir)?
            },
        };
        report.phase_timings_ms.insert(
            "open_search_index_ms".to_string(),
            elapsed_ms(search_started),
        );

        let store = Self {
            index,
            search,
            schema_registry,
            index_root: index_root.clone(),
            hook: OnceLock::new(),
        };
        let add_root_started = Instant::now();
        store.add_scan_root(ScanRoot {
            path: index_root.join("tickets"),
            label: "tickets".to_string(),
        })?;
        report.phase_timings_ms.insert(
            "add_default_scan_root_ms".to_string(),
            elapsed_ms(add_root_started),
        );
        let bootstrap_started = Instant::now();
        let bootstrap = store.bootstrap_empty_index_from_manifests_profiled()?;
        report.phase_timings_ms.insert(
            "bootstrap_empty_index_ms".to_string(),
            elapsed_ms(bootstrap_started),
        );
        merge_prefixed_timings(
            &mut report.phase_timings_ms,
            "bootstrap",
            bootstrap.phase_timings_ms,
        );
        report.scan_reports.extend(bootstrap.scan_reports);
        report.phase_timings_ms.insert(
            "open_internal_total_ms".to_string(),
            elapsed_ms(overall_started),
        );
        emit_store_open_report("ticket_store_open_internal_complete", &report);
        Ok((store, report))
    }

    fn bootstrap_empty_index_from_manifests_profiled(
        &self,
    ) -> Result<StoreOpenReport, StorageError> {
        let _span_guard = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_bootstrap_empty_index"
        )
        .entered();
        let mut report = StoreOpenReport::default();
        let count_started = Instant::now();
        if self.count_tickets()? > 0 {
            report.phase_timings_ms.insert(
                "count_tickets_ms".to_string(),
                elapsed_ms(count_started),
            );
            let search_probe_started = Instant::now();
            if self
                .search
                .search(&crate::model::query::Expr::And(Vec::new()), 1)
                .map(|results| results.is_empty())
                .unwrap_or(true)
            {
                report.phase_timings_ms.insert(
                    "search_probe_ms".to_string(),
                    elapsed_ms(search_probe_started),
                );
                let bootstrap_scan_started = Instant::now();
                let scan_report = self.scan(true)?;
                report.phase_timings_ms.insert(
                    "scan_existing_index_ms".to_string(),
                    elapsed_ms(bootstrap_scan_started),
                );
                report
                    .scan_reports
                    .insert("bootstrap_existing_index_scan".to_string(), scan_report);
                return Ok(report);
            }
            report.phase_timings_ms.insert(
                "search_probe_ms".to_string(),
                elapsed_ms(search_probe_started),
            );
            return Ok(report);
        }
        report.phase_timings_ms.insert(
            "count_tickets_ms".to_string(),
            elapsed_ms(count_started),
        );

        let roots_started = Instant::now();
        for root in self.list_scan_roots()? {
            report.phase_timings_ms.insert(
                "list_scan_roots_ms".to_string(),
                elapsed_ms(roots_started),
            );
            let manifest_probe_started = Instant::now();
            if scan_root_has_ticket_manifests(&root.path)? {
                report.phase_timings_ms.insert(
                    "manifest_probe_ms".to_string(),
                    elapsed_ms(manifest_probe_started),
                );
                let bootstrap_scan_started = Instant::now();
                let scan_report = self.scan(true)?;
                report.phase_timings_ms.insert(
                    "scan_manifest_bootstrap_ms".to_string(),
                    elapsed_ms(bootstrap_scan_started),
                );
                report
                    .scan_reports
                    .insert("bootstrap_manifest_scan".to_string(), scan_report);
                break;
            }
            report.phase_timings_ms.insert(
                "manifest_probe_ms".to_string(),
                elapsed_ms(manifest_probe_started),
            );
        }

        if !report.phase_timings_ms.contains_key("list_scan_roots_ms") {
            report.phase_timings_ms.insert(
                "list_scan_roots_ms".to_string(),
                elapsed_ms(roots_started),
            );
        }

        Ok(report)
    }

    /// Access the schema registry to look up type schemas.
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
        let mut indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;

        // Determine the target state and transition path.
        // Priority: to_state > last element of transition_states > current state (no change)
        let (new_state, transition_path) = if let Some(to) = to_state {
            let path = self.resolve_transition_path(
                &indexed,
                transition_states.unwrap_or(&[]),
                to,
            )?;
            let final_state = path
                .last()
                .cloned()
                .unwrap_or_else(|| to.to_string());
            (Some(final_state), path)
        } else if let Some(transition_states_slice) = transition_states {
            // transition_states provided but no to_state: use last element as target
            if let Some(final_target) = transition_states_slice.last() {
                // Pass all elements except the last as intermediate steps
                let intermediate_steps = &transition_states_slice[..transition_states_slice.len() - 1];
                let path = self.resolve_transition_path(
                    &indexed,
                    intermediate_steps,
                    final_target,
                )?;
                (Some(final_target.clone()), path)
            } else {
                // Empty transition_states array: no change
                (indexed.state.clone(), Vec::new())
            }
        } else {
            // No to_state and no transition_states: preserve current state
            (indexed.state.clone(), Vec::new())
        };
        let previous_state = indexed.state.clone();
        let updated_manifest = if transition_path.is_empty() {
            TicketFs::update(&indexed.path, &patch, new_state.as_deref())?
        } else {
            let mut manifest = None;
            for (index, state) in transition_path.iter().enumerate() {
                let step_patch = if index + 1 == transition_path.len() {
                    patch.clone()
                } else {
                    BTreeMap::new()
                };
                manifest = Some(TicketFs::update(
                    &indexed.path,
                    &step_patch,
                    Some(state.as_str()),
                )?);
            }
            manifest.expect("transition path produces at least one manifest")
        };

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
        let created_at_str = indexed.created_at.to_rfc3339();
        let effort_str = updated_manifest.extra.get("effort")
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

        Ok(updated_manifest)
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

fn emit_store_open_report(
    event_name: &'static str,
    report: &StoreOpenReport,
) {
    tracing::debug!(
        target: STORE_TRACE_TARGET,
        initialized_store = report.initialized_store,
        phase_count = report.phase_timings_ms.len(),
        scan_report_count = report.scan_reports.len(),
        "{event_name}"
    );
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

fn merge_timings(
    target: &mut BTreeMap<String, u64>,
    source: BTreeMap<String, u64>,
) {
    target.extend(source);
}

fn merge_prefixed_timings(
    target: &mut BTreeMap<String, u64>,
    prefix: &str,
    source: BTreeMap<String, u64>,
) {
    for (key, value) in source {
        target.insert(format!("{prefix}_{key}"), value);
    }
}

fn scan_root_has_ticket_manifests(root: &Path) -> Result<bool, StorageError> {
    if !root.exists() {
        return Ok(false);
    }

    for entry in fs::read_dir(root).map_err(StorageError::Io)? {
        let path = entry.map_err(StorageError::Io)?.path();
        if path.is_dir() && path.join(TICKET_MANIFEST_FILE).is_file() {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::*;
    use crate::model::filesystem::ScanRoot;

    #[test]
    fn recovers_ticket_paths_from_relative_index_entries() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let store_root = repo.join("viewer").join(".ticket");
        std::fs::create_dir_all(&store_root).unwrap();

        let store = TicketStore::init(&store_root).unwrap();
        let ticket_id = store
            .create(
                None,
                "tracker-improvement",
                Some("Recover relative ticket paths"),
                None,
                Default::default(),
                None,
                Some("Detailed context for recovery test."),
            )
            .unwrap();

        let absolute_scan_root = store.index_root.join("tickets");
        let absolute_ticket_path =
            absolute_scan_root.join(ticket_id.to_string());
        let relative_scan_root = PathBuf::from("viewer/.ticket/tickets");

        store
            .index
            .add_scan_root(&ScanRoot {
                path: relative_scan_root.clone(),
                label: "relative".to_string(),
            })
            .unwrap();

        let mut indexed = store.index.get_ticket(&ticket_id).unwrap().unwrap();
        indexed.path = relative_scan_root.join(ticket_id.to_string());
        store.index.insert_ticket(&indexed).unwrap();

        let roots = store.list_scan_roots().unwrap();
        assert_eq!(
            roots
                .iter()
                .filter(|root| root.path == absolute_scan_root)
                .count(),
            1
        );
        assert!(roots.iter().all(|root| root.path.is_absolute()));

        let indexed = store.get_indexed(&ticket_id).unwrap().unwrap();
        assert_eq!(indexed.path, absolute_ticket_path);
        assert_eq!(
            TicketFs::read_description(&indexed.path).as_deref(),
            Some("Detailed context for recovery test.")
        );
        assert!(store.get(&ticket_id).is_ok());
    }
}
