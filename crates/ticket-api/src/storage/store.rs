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
mod parts;
mod query;
mod release;
mod scan;
mod store_open;
mod workflow_facts;

pub use self::{
    parts::{
        PART_HISTORY_CONTENT_KEY,
        PART_HISTORY_ID_KEY,
    },
    release::{
        GateCheckOutcome,
        GateStatus,
        PromoteOutcome,
        ValidationResultOutcome,
    },
    scan::ScanReport,
};

const STORE_TRACE_TARGET: &str = "ticket_api::storage::store";

/// History field key under which the pre-update `description.md` content is
/// captured whenever a description change is applied, regardless of
/// [`DescriptionUpdateMode`]. Used by [`TicketStore::apply_revert`] to
/// restore the description on undo.
pub const DESCRIPTION_HISTORY_KEY: &str = "__previous_description__";

/// How a caller-supplied `description` value should be applied to an
/// existing ticket's `description.md`.
///
/// Has no default: every caller supplying `description` must state a mode
/// explicitly (see [`REQUIRED_DESCRIPTION_MODE_ERROR`]). The historical
/// silent `Replace` default was the direct cause of destructive overwrites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptionUpdateMode {
    /// Overwrite `description.md` with the supplied text.
    Replace,
    /// Concatenate the supplied text onto the existing description.
    Append,
}

/// Error text returned when `description` is supplied with no
/// `description_mode` (AC1/AC2 of ticket 3d952036). Names both modes and
/// states which one preserves existing content.
pub const REQUIRED_DESCRIPTION_MODE_ERROR: &str = "update_ticket with `description` requires an explicit `description_mode`: 'replace' (overwrites description.md) or 'append' (preserves existing content by concatenating the new text onto it). There is no default; omitting description_mode is rejected.";

/// A caller's requested change to `description.md`, bundling the mode with
/// its content so "content supplied without a mode" is unrepresentable at
/// the type level: there is no separate `description_mode` field to omit
/// once a value of this type exists (AC5 of ticket 3d952036). Boundary
/// transports (HTTP/MCP JSON, CLI flags) decode two raw wire fields into
/// this type via [`DescriptionUpdate::decode`]; every Rust construction
/// downstream of that single decode point must pick one of these variants
/// explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptionUpdate {
    /// Leave `description.md` untouched.
    Unchanged,
    /// Overwrite `description.md` with the given text.
    Replace(String),
    /// Concatenate the given text onto the existing description.
    Append(String),
}

impl DescriptionUpdate {
    /// Boundary decoder: the one place a raw wire `description_mode` string
    /// is interpreted. Returns [`REQUIRED_DESCRIPTION_MODE_ERROR`] when
    /// `description` is set without a recognized mode string, and a named
    /// error for an unrecognized mode string.
    pub fn decode(
        description: Option<String>,
        description_mode: Option<&str>,
    ) -> Result<Self, String> {
        match description {
            None => Ok(DescriptionUpdate::Unchanged),
            Some(text) => match description_mode {
                Some("replace") => Ok(DescriptionUpdate::Replace(text)),
                Some("append") => Ok(DescriptionUpdate::Append(text)),
                Some(other) => Err(format!(
                    "invalid description_mode '{other}': expected 'replace' or 'append'"
                )),
                None => Err(REQUIRED_DESCRIPTION_MODE_ERROR.to_string()),
            },
        }
    }

    /// Split back into the `(content, mode)` pair
    /// [`TicketStore::update_with_options`] expects. Infallible: every
    /// variant already encodes a valid combination, so this can never hit
    /// the runtime `description_mode.ok_or_else(..)` guard in
    /// `apply_manifest_update` — that guard remains only as defense in depth
    /// for any caller that bypasses [`DescriptionUpdate`] entirely.
    pub fn as_parts(&self) -> (Option<&str>, Option<DescriptionUpdateMode>) {
        match self {
            DescriptionUpdate::Unchanged => (None, None),
            DescriptionUpdate::Replace(text) => {
                (Some(text.as_str()), Some(DescriptionUpdateMode::Replace))
            },
            DescriptionUpdate::Append(text) => {
                (Some(text.as_str()), Some(DescriptionUpdateMode::Append))
            },
        }
    }
}

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
        let state = initial_state.unwrap_or("open").to_string();
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
        let effort_str = manifest.extra.get("effort").and_then(|v| match v {
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
        if let Err(error) = TicketFs::append_history(
            &indexed.path,
            manifest.extra.clone(),
            None,
        ) {
            tracing::error!(
                ticket_id = %id,
                path = %indexed.path.display(),
                %error,
                "failed to append history revision; manifest write succeeded but undo history is now incomplete"
            );
        }

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
    ) -> Result<Option<crate::storage::indexed::WorkflowFacts>, StorageError>
    {
        self.index.get_workflow_facts(id)
    }

    pub fn get_workflow_facts_many(
        &self,
        ids: &[Uuid],
    ) -> Result<
        HashMap<Uuid, crate::storage::indexed::WorkflowFacts>,
        StorageError,
    > {
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
        self.update_with_options(
            id,
            patch,
            transition_states,
            to_state,
            description,
            None,
            author,
            false,
        )
    }

    /// Same as [`update`](Self::update) but with an explicit `single_hop`
    /// opt-out. When `single_hop` is `false` (the default used by [`update`]),
    /// a reachable multi-hop `to_state` is auto-walked through its required
    /// intermediate states. When `single_hop` is `true`, a transition that
    /// would skip a required waypoint is rejected with a recovery-oriented
    /// [`SchemaValidationError::InvalidTransition`] instead of being walked.
    #[allow(clippy::too_many_arguments)]
    pub fn update_with_options(
        &self,
        id: &Uuid,
        patch: BTreeMap<String, Value>,
        transition_states: Option<&[String]>,
        to_state: Option<&str>,
        description: Option<&str>,
        description_mode: Option<DescriptionUpdateMode>,
        author: Option<&str>,
        single_hop: bool,
    ) -> Result<TicketManifest, StorageError> {
        let mut patch = patch;

        // `state` must never be applied as a plain field patch: doing so
        // would either bypass transition validation (if silently applied)
        // or be silently dropped (as it previously was, when the
        // to_state-derived `new_state` overwrote the patched value with the
        // ticket's unchanged current state). Reject it explicitly so callers
        // route state changes through `to_state`/`transition_states`, which
        // validate against the schema's allowed transitions.
        if patch.contains_key("state") {
            return Err(StorageError::Other(format!(
                "field 'state' cannot be set via a field/field_map patch (rejected value: {:?}); use to_state or transition_states to change ticket state",
                patch.get("state")
            )));
        }

        let mut indexed =
            self.get_indexed(id)?.ok_or(StorageError::NotFound(*id))?;
        let current_manifest = TicketFs::read(&indexed.path)?;
        let edge_patch_plans =
            edge_patch_plans(&patch, &current_manifest.extra)?;
        strip_file_backed_edge_fields(&mut patch);

        // Determine the target state and transition path.
        // Priority: to_state > last element of transition_states > current state (no change)
        let (new_state, transition_path) = self.resolve_update_target(
            &indexed,
            transition_states,
            to_state,
            single_hop,
        )?;
        let previous_state = indexed.state.clone();
        let (updated_manifest, previous_description) = self
            .apply_manifest_update(
                id,
                &indexed.path,
                &patch,
                &new_state,
                &transition_path,
                &indexed.type_id,
                description,
                description_mode,
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

        // Append history snapshot after successful write. Always capture the
        // pre-update description alongside the manifest fields whenever a
        // description change was applied, regardless of mode, so it is
        // recoverable via undo.
        //
        // A failure here is logged loudly rather than propagated: the
        // manifest/description write above already succeeded and is the
        // system of record, so failing the whole update would report
        // failure for a mutation that actually committed (and could prompt
        // a caller retry that double-applies the patch). The history file
        // is a best-effort undo trail; losing an entry must be visible, not
        // silent, but must not mask a successful write as an error.
        let mut history_fields = updated_manifest.extra.clone();
        if description.is_some() {
            history_fields.insert(
                DESCRIPTION_HISTORY_KEY.to_string(),
                previous_description
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
        }
        if let Err(error) = TicketFs::append_history(
            &indexed.path,
            history_fields,
            author.map(str::to_string),
        ) {
            tracing::error!(
                ticket_id = %id,
                path = %indexed.path.display(),
                %error,
                "failed to append history revision; manifest write succeeded but undo history is now incomplete"
            );
        }


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

        let state_progressed = previous_state.as_deref()
            != new_state.as_deref()
            && self.state_rank_for_type(&indexed.type_id, new_state.as_deref())
                > self.state_rank_for_type(
                    &indexed.type_id,
                    previous_state.as_deref(),
                );
        if previous_state.as_deref() != new_state.as_deref() {
            self.refresh_workflow_facts_for_roots(
                &[*id],
                state_progressed,
                now,
            )?;
        }

        Ok(TicketFs::read(&indexed.path).unwrap_or(updated_manifest))
    }

    fn resolve_update_target(
        &self,
        indexed: &IndexedTicket,
        transition_states: Option<&[String]>,
        to_state: Option<&str>,
        single_hop: bool,
    ) -> Result<(Option<String>, Vec<String>), StorageError> {
        if let Some(to) = to_state {
            let path = self.resolve_transition_path(
                indexed,
                transition_states.unwrap_or(&[]),
                to,
                single_hop,
            )?;
            let final_state =
                path.last().cloned().unwrap_or_else(|| to.to_string());
            return Ok((Some(final_state), path));
        }

        if let Some(transition_states_slice) = transition_states {
            if let Some(final_target) = transition_states_slice.last() {
                let intermediate_steps = &transition_states_slice
                    [..transition_states_slice.len() - 1];
                let path = self.resolve_transition_path(
                    indexed,
                    intermediate_steps,
                    final_target,
                    single_hop,
                )?;
                return Ok((Some(final_target.clone()), path));
            }
            return Ok((indexed.state.clone(), Vec::new()));
        }

        Ok((indexed.state.clone(), Vec::new()))
    }

    fn apply_manifest_update(
        &self,
        id: &Uuid,
        ticket_path: &Path,
        patch: &BTreeMap<String, Value>,
        new_state: &Option<String>,
        transition_path: &[String],
        type_id: &str,
        description: Option<&str>,
        description_mode: Option<DescriptionUpdateMode>,
    ) -> Result<(TicketManifest, Option<String>), StorageError> {
        // Gate on the ticket's state as of the start of this call, before
        // any transition below applies this same call's freeze/unfreeze.
        // A single call that both transitions into `planned` and sets the
        // description is the freeze taking effect, not a write to a part
        // already frozen by a prior call — AC7 targets the latter only
        // (proven by `f9e70385_legacy_description_write_rejected_when_objective_frozen`,
        // which freezes in one call and writes in a separate later one).
        if description.is_some() {
            self.enforce_description_write_gate(id)?;
        }

        let updated_manifest = if transition_path.is_empty() {
            TicketFs::update(ticket_path, patch, new_state.as_deref())?
        } else {
            // Plan freezing (spec 24b3d22b, ticket f9e70385, AC1/AC5): every
            // state visited along the transition path is evaluated. Entering
            // `planned` freezes the five planning parts (materializing any
            // missing) and cuts a plan revision; landing on any state ranked
            // below `planned` clears every frozen flag. States ranked at or
            // above `planned` otherwise leave frozen flags untouched.
            let planned_rank =
                self.state_rank_for_type(type_id, Some("planned"));
            let mut manifest = None;
            for (index, state) in transition_path.iter().enumerate() {
                let step_patch = if index + 1 == transition_path.len() {
                    patch.clone()
                } else {
                    BTreeMap::new()
                };
                let mut step_manifest = TicketFs::update(
                    ticket_path,
                    &step_patch,
                    Some(state.as_str()),
                )?;
                if state.as_str() == "planned" {
                    step_manifest =
                        TicketFs::apply_plan_freeze(ticket_path, true)?;
                } else if self.state_rank_for_type(type_id, Some(state.as_str()))
                    < planned_rank
                {
                    step_manifest =
                        TicketFs::apply_plan_freeze(ticket_path, false)?;
                }
                manifest = Some(step_manifest);
            }
            manifest.expect("transition path produces at least one manifest")
        };

        let mut previous_description = None;
        if let Some(desc) = description {
            // AC1/AC2: an omitted description_mode is a hard error, not a
            // silent default — the silent `Replace` default was the direct
            // cause of destructive description overwrites.
            let mode = description_mode.ok_or_else(|| {
                StorageError::Other(REQUIRED_DESCRIPTION_MODE_ERROR.to_string())
            })?;
            let existing = TicketFs::read_description(ticket_path);
            previous_description = existing.clone();
            let final_text = match mode {
                DescriptionUpdateMode::Replace => desc.to_string(),
                DescriptionUpdateMode::Append => match existing {
                    Some(existing) if !existing.is_empty() => {
                        format!("{existing}\n{desc}")
                    },
                    _ => desc.to_string(),
                },
            };
            // AC7: the legacy `description` write is not a privileged
            // bypass of plan freezing — it was already gated above against
            // this call's pre-transition state.
            TicketFs::write_description(ticket_path, &final_text)?;
        }

        Ok((updated_manifest, previous_description))
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
        let effort_str =
            updated_manifest.extra.get("effort").and_then(|v| match v {
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
        single_hop: bool,
    ) -> Result<Vec<String>, StorageError> {
        let current_state = indexed.state.as_deref().unwrap_or("open");
        if current_state == target_state && transition_states.is_empty() {
            return Ok(vec![]);
        }
        let schema =
            self.schema_registry.get(&indexed.type_id).ok_or_else(|| {
                StorageError::Other(format!(
                    "no schema for type '{}'",
                    indexed.type_id
                ))
            })?;

        self.enforce_dependency_progress(indexed, target_state)?;

        let mut path = Vec::new();
        let mut from = current_state.to_string();
        let mut checkpoints: Vec<String> = transition_states.to_vec();
        checkpoints.push(target_state.to_string());

        for checkpoint in checkpoints {
            if checkpoint == from {
                continue;
            }

            match schema.find_path(&from, &checkpoint) {
                // Reachable path. By default auto-walk it, visiting every
                // required intermediate state. Under the `single_hop` opt-out,
                // a multi-hop path that would skip a required waypoint is
                // rejected with recovery guidance instead of being walked.
                Some(segment) => {
                    if single_hop && segment.len() > 1 {
                        return Err(StorageError::Validation(
                            crate::error::SchemaValidationError::InvalidTransition {
                                from: from.clone(),
                                to: checkpoint.clone(),
                                allowed_next: schema.allowed_next_states(&from),
                                intermediate: segment,
                            },
                        ));
                    }
                    path.extend(segment);
                    from = checkpoint;
                },
                // No path exists at all: the target is unreachable from here.
                None => {
                    return Err(StorageError::Validation(
                        schema.invalid_transition_error(&from, &checkpoint),
                    ));
                },
            }
        }

        if !schema.required_states.is_empty()
            && schema.terminal_states.contains(&target_state.to_string())
        {
            let history =
                TicketFs::read_history(&indexed.path).unwrap_or_default();
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

    /// Guard: a ticket may not advance further along the state schema than any
    /// of its unresolved `depends_on` targets. Terminal (`done`/`cancelled`)
    /// dependencies are treated as satisfied, cross-store dependencies that are
    /// not locally resolvable are skipped, and cancelling a ticket is always
    /// permitted regardless of dependency progress.
    fn enforce_dependency_progress(
        &self,
        indexed: &IndexedTicket,
        target_state: &str,
    ) -> Result<(), StorageError> {
        if target_state == "cancelled" {
            return Ok(());
        }
        let target_rank =
            self.state_rank_for_type(&indexed.type_id, Some(target_state));
        for edge in self.edges_from(&indexed.id)? {
            if edge.kind != "depends_on" {
                continue;
            }
            let Some(dependency) = self.get_indexed(&edge.to)? else {
                continue;
            };
            let dependency_state = dependency.state.as_deref();
            if matches!(dependency_state, Some("done") | Some("cancelled")) {
                continue;
            }
            let dependency_rank =
                self.state_rank_for_type(&dependency.type_id, dependency_state);
            if target_rank > dependency_rank {
                return Err(StorageError::DependencyNotProgressed {
                    ticket: indexed.id,
                    target_state: target_state.to_string(),
                    dependency: edge.to,
                    dependency_state: dependency_state
                        .unwrap_or("open")
                        .to_string(),
                });
            }
        }
        Ok(())
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
