use std::{
    collections::HashSet,
    fs,
    path::Path,
    time::Instant,
};

use chrono::Utc;
use tracing::field::Empty;
use uuid::Uuid;

use crate::{
    error::StorageError,
    model::{
        edge::EdgeRecord,
        filesystem::{
            ParseDiagnostic,
            PersistedScanRoot,
            PolicyDecision,
            ScanRoot,
            ScanRootMetadata,
            ScanRootSource,
            TICKET_MANIFEST_FILE,
        },
    },
    storage::{
        index::RedbIndexStore,
        indexed::IndexedTicket,
        search::SearchDocumentInput,
        ticket_fs::{
            TicketFs,
            TicketScanEntry,
        },
    },
};

use super::TicketStore;

const FILE_BACKED_EDGE_KINDS: &[&str] = &["depends_on", "linked"];
const STORE_TRACE_TARGET: &str = "ticket_api::storage::store";

#[derive(Debug, Clone, Default)]
pub struct ScanReport {
    pub integrated: usize,
    pub pruned: usize,
    pub diagnostics: Vec<ParseDiagnostic>,
    pub phase_timings_ms: std::collections::BTreeMap<String, u64>,
    pub root_entry_counts: std::collections::BTreeMap<String, usize>,
    /// Labels of scan roots skipped because policy marked them `ignored`.
    pub skipped_roots: Vec<String>,
}

impl TicketStore {
    pub(super) fn is_external_worktree_path(
        &self,
        path: &Path,
    ) -> bool {
        match (
            Self::worktree_scope(path),
            Self::worktree_scope(&self.index_root),
        ) {
            (Some(path_scope), Some(store_scope)) => path_scope != store_scope,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    fn worktree_scope(path: &Path) -> Option<std::path::PathBuf> {
        let normalized = Self::normalize_path(path.to_path_buf());
        let mut scope = std::path::PathBuf::new();
        let mut found_worktrees = false;

        for component in normalized.components() {
            scope.push(component.as_os_str());
            if found_worktrees {
                return Some(scope);
            }
            found_worktrees = component.as_os_str() == ".worktrees";
        }

        None
    }

    pub fn add_scan_root(
        &self,
        root: ScanRoot,
    ) -> Result<(), StorageError> {
        let path = self.resolve_scan_root_path(&root.path);
        if self.is_external_worktree_path(&path) {
            return Err(StorageError::Other(format!(
                "refusing scan root under .worktrees outside store root: {}",
                path.display()
            )));
        }
        self.index.add_scan_root(&ScanRoot {
            path,
            label: root.label,
        })?;
        Ok(())
    }

    /// Persist a scan root together with explicit auditability metadata.
    pub fn add_scan_root_with_metadata(
        &self,
        root: ScanRoot,
        metadata: ScanRootMetadata,
    ) -> Result<(), StorageError> {
        let path = self.resolve_scan_root_path(&root.path);
        if self.is_external_worktree_path(&path) {
            return Err(StorageError::Other(format!(
                "refusing scan root under .worktrees outside store root: {}",
                path.display()
            )));
        }
        self.index.add_scan_root_with_metadata(
            &ScanRoot {
                path,
                label: root.label,
            },
            &metadata,
        )?;
        Ok(())
    }

    /// Delete persisted sibling-worktree scan roots that can outlive the
    /// worktree and leave stale indexed ticket paths behind.
    pub fn prune_worktree_scan_roots(
        &self
    ) -> Result<Vec<ScanRoot>, StorageError> {
        let persisted = self.index.list_scan_roots()?;
        let paths: Vec<_> = persisted
            .iter()
            .filter_map(|root| {
                let path = self.resolve_scan_root_path(&root.path);
                self.is_external_worktree_path(&path)
                    .then_some(root.path.clone())
            })
            .collect();
        self.index.remove_scan_roots(&paths)?;

        Ok(persisted
            .into_iter()
            .filter(|root| paths.iter().any(|path| path == &root.path))
            .map(|root| ScanRoot {
                path: self.resolve_scan_root_path(&root.path),
                label: root.label,
            })
            .collect())
    }

    pub fn list_scan_roots(&self) -> Result<Vec<ScanRoot>, StorageError> {
        let mut seen = HashSet::new();
        let mut roots = Vec::new();

        for root in self.index.list_scan_roots()? {
            let path = self.resolve_scan_root_path(&root.path);
            if seen.insert(path.clone()) {
                roots.push(ScanRoot {
                    path,
                    label: root.label,
                });
            }
        }

        Ok(roots)
    }

    /// List persisted scan roots together with their auditability metadata.
    ///
    /// Paths are resolved and de-duplicated exactly like [`list_scan_roots`].
    pub fn list_scan_roots_with_metadata(
        &self
    ) -> Result<Vec<PersistedScanRoot>, StorageError> {
        let mut seen = HashSet::new();
        let mut roots = Vec::new();

        for persisted in self.index.list_scan_roots_with_metadata()? {
            let path = self.resolve_scan_root_path(&persisted.root.path);
            if seen.insert(path.clone()) {
                roots.push(PersistedScanRoot {
                    root: ScanRoot {
                        path,
                        label: persisted.root.label,
                    },
                    metadata: persisted.metadata,
                });
            }
        }

        Ok(roots)
    }

    /// Re-apply the workspace policy to the registered scan roots.
    ///
    /// Discovery is re-run under the resolved policy: every discovered allowed
    /// root is (re-)registered with `policy_decision = included`, and any
    /// previously-registered root that the policy no longer allows is flipped to
    /// `policy_decision = ignored` (so scan-time skipping and the query-time
    /// guard exclude it). A forced rescan then re-indexes, and the returned
    /// [`ScanReport`] surfaces the skipped roots.
    pub fn reapply_workspace_policy(
        &self,
        workspace_root: &Path,
    ) -> Result<ScanReport, StorageError> {
        let policy =
            crate::workspace_policy::load_workspace_policy(workspace_root);

        let allowed =
            crate::workspace::discover_workspace_scan_roots_with_policy(
                workspace_root,
                crate::workspace::TICKET_INDEX_DIR,
                "tickets",
                &policy,
            );
        let allowed_paths: HashSet<std::path::PathBuf> = allowed
            .iter()
            .map(|root| self.resolve_scan_root_path(&root.path))
            .collect();

        // Flip previously-registered roots the policy no longer allows.
        for persisted in self.list_scan_roots_with_metadata()? {
            if !allowed_paths.contains(&persisted.root.path) {
                self.add_scan_root_with_metadata(
                    persisted.root,
                    ScanRootMetadata {
                        source: ScanRootSource::Policy,
                        policy_decision: PolicyDecision::Ignored,
                        workspace_root: Some(workspace_root.to_path_buf()),
                    },
                )?;
            }
        }

        // (Re-)register every allowed root as policy-included.
        for root in allowed {
            self.add_scan_root_with_metadata(
                root,
                ScanRootMetadata {
                    source: ScanRootSource::Policy,
                    policy_decision: PolicyDecision::Included,
                    workspace_root: Some(workspace_root.to_path_buf()),
                },
            )?;
        }

        self.scan(true)
    }

    pub fn scan(
        &self,
        reindex: bool,
    ) -> Result<ScanReport, StorageError> {
        let span = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_scan",
            requested_reindex = reindex,
            forced_reindex = Empty,
        );
        let _span_guard = span.enter();
        let overall_started = Instant::now();
        let search_rebuild_started = Instant::now();
        // Proactively enforce all search-index invariants before any write. The
        // rebuild check heals structural corruption (via `num_docs`) and detects
        // an empty/partial/unreadable index; either forces a full rebuild so the
        // search index is reset and repopulated from the on-disk tickets.
        let force = reindex || self.search_needs_rebuild()?;
        let search_rebuild_elapsed = elapsed_ms(search_rebuild_started);
        span.record("forced_reindex", force);
        let mut report = self.scan_once(force)?;
        report.phase_timings_ms.insert(
            "search_rebuild_check_ms".to_string(),
            search_rebuild_elapsed,
        );
        record_phase_timing(
            &mut report.phase_timings_ms,
            "scan_total_ms",
            overall_started,
        );
        tracing::debug!(
            target: STORE_TRACE_TARGET,
            integrated = report.integrated,
            pruned = report.pruned,
            diagnostics = report.diagnostics.len(),
            scan_roots = report.root_entry_counts.len(),
            "ticket_store_scan_complete"
        );
        Ok(report)
    }

    pub fn reconcile_known_tickets(
        &self,
        ticket_ids: &[Uuid],
    ) -> Result<ScanReport, StorageError> {
        let overall_started = Instant::now();
        let mut phase_timings_ms = std::collections::BTreeMap::new();
        phase_timings_ms.insert(
            "targeted_reconcile_known_count".to_string(),
            ticket_ids.len() as u64,
        );

        let search_rebuild_started = Instant::now();
        let force = self.search_needs_rebuild()?;
        phase_timings_ms.insert(
            "search_rebuild_check_ms".to_string(),
            elapsed_ms(search_rebuild_started),
        );
        if force {
            let mut report = self.scan_once(true)?;
            merge_phase_totals(&mut report.phase_timings_ms, phase_timings_ms);
            record_phase_timing(
                &mut report.phase_timings_ms,
                "scan_total_ms",
                overall_started,
            );
            return Ok(report);
        }

        let list_roots_started = Instant::now();
        let mut roots = self.list_scan_roots()?;
        let default_root = ScanRoot {
            path: self.resolve_scan_root_path(&self.index_root.join("tickets")),
            label: "default".to_string(),
        };
        if !roots.iter().any(|root| root.path == default_root.path) {
            roots.insert(0, default_root);
        }
        record_phase_timing(
            &mut phase_timings_ms,
            "list_scan_roots_ms",
            list_roots_started,
        );

        let mut unique_ids = ticket_ids.to_vec();
        unique_ids.sort_unstable();
        unique_ids.dedup();

        let targeted_started = Instant::now();
        let mut integrated = 0usize;
        let mut pruned = 0usize;
        let mut diagnostics = Vec::new();
        let mut root_entry_counts = std::collections::BTreeMap::new();
        root_entry_counts.insert("known_ids".to_string(), unique_ids.len());

        let mut indexed_updates = Vec::with_capacity(unique_ids.len());
        let mut edge_updates = Vec::new();
        let mut search_documents = Vec::new();
        let mut stale_ids = Vec::new();
        let mut workflow_root_ids = HashSet::new();

        for id in unique_ids {
            let existing = self.get_indexed(&id)?;
            let entry =
                resolve_known_ticket_entry(id, existing.as_ref(), &roots)?;
            match entry {
                Some(entry) => {
                    integrated += 1;
                    if let Some(update) =
                        integrate_entry(&self.index, entry, false)?
                    {
                        *phase_timings_ms
                            .entry(
                                "integration.description_read_ms".to_string(),
                            )
                            .or_insert(0) += update.description_read_ms;
                        indexed_updates.push(update.indexed);
                        edge_updates.extend(update.edges);
                        search_documents.push(update.search_document);
                        workflow_root_ids.insert(id);
                    }
                },
                None =>
                    if let Some(ticket) = existing {
                        diagnostics.push(stale_reconciliation_diagnostic(
                            &ticket, &roots,
                        ));
                        stale_ids.push(id);
                        workflow_root_ids.insert(id);
                        pruned += 1;
                    },
            }
        }

        let index_upsert_started = Instant::now();
        self.index.upsert_tickets_batch(&indexed_updates)?;
        add_phase_elapsed(
            &mut phase_timings_ms,
            "integration.index_upsert_ms",
            index_upsert_started,
        );

        let edge_write_started = Instant::now();
        self.index.insert_edges_batch(&edge_updates)?;
        add_phase_elapsed(
            &mut phase_timings_ms,
            "integration.edge_write_ms",
            edge_write_started,
        );

        let search_upsert_started = Instant::now();
        self.search.upsert_batch(&search_documents)?;
        add_phase_elapsed(
            &mut phase_timings_ms,
            "integration.search_upsert_ms",
            search_upsert_started,
        );

        let prune_started = Instant::now();
        self.index.remove_tickets_batch(&stale_ids)?;
        self.search.remove_batch(&stale_ids)?;
        record_phase_timing(
            &mut phase_timings_ms,
            "prune_stale_ms",
            prune_started,
        );

        let workflow_started = Instant::now();
        let mut roots = workflow_root_ids.into_iter().collect::<Vec<_>>();
        roots.sort_unstable();
        let workflow_timings = self
            .refresh_workflow_facts_for_roots_with_timings(
                &roots,
                false,
                Utc::now(),
            )?;
        merge_phase_totals(&mut phase_timings_ms, workflow_timings);
        record_phase_timing(
            &mut phase_timings_ms,
            "rebuild_workflow_facts_ms",
            workflow_started,
        );

        record_phase_timing(
            &mut phase_timings_ms,
            "targeted_reconcile_known_ms",
            targeted_started,
        );
        record_phase_timing(
            &mut phase_timings_ms,
            "scan_total_ms",
            overall_started,
        );

        Ok(ScanReport {
            integrated,
            pruned,
            diagnostics,
            phase_timings_ms,
            root_entry_counts,
            skipped_roots: Vec::new(),
        })
    }

    fn scan_once(
        &self,
        reindex: bool,
    ) -> Result<ScanReport, StorageError> {
        let _span_guard = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_scan_once",
            reindex,
        )
        .entered();
        let mut phase_timings_ms = std::collections::BTreeMap::new();
        let mut root_entry_counts = std::collections::BTreeMap::new();
        if reindex {
            let backfill_started = Instant::now();
            self.backfill_file_backed_edges_from_index()?;
            record_phase_timing(
                &mut phase_timings_ms,
                "backfill_file_backed_edges_ms",
                backfill_started,
            );
            let reset_started = Instant::now();
            // Reset the directory instead of clearing documents: a forced
            // rebuild must not depend on opening the (possibly corrupt) existing
            // index. The next upsert recreates a fresh index from the current
            // schema.
            self.search.reset_dir()?;
            record_phase_timing(
                &mut phase_timings_ms,
                "reset_search_index_ms",
                reset_started,
            );
            let clear_edges_started = Instant::now();
            self.index.clear_edges()?;
            record_phase_timing(
                &mut phase_timings_ms,
                "clear_index_edges_ms",
                clear_edges_started,
            );
        }

        let list_roots_started = Instant::now();
        let mut skipped_roots = Vec::new();
        let mut roots = Vec::new();
        for persisted in self.list_scan_roots_with_metadata()? {
            if persisted.metadata.policy_decision.is_ignored() {
                tracing::debug!(
                    target: STORE_TRACE_TARGET,
                    root_label = %persisted.root.label,
                    "ticket_store_scan_root_skipped_by_policy"
                );
                skipped_roots.push(persisted.root.label);
                continue;
            }
            roots.push(persisted.root);
        }
        record_phase_timing(
            &mut phase_timings_ms,
            "list_scan_roots_ms",
            list_roots_started,
        );
        let default_root = ScanRoot {
            path: self.resolve_scan_root_path(&self.index_root.join("tickets")),
            label: "default".to_string(),
        };
        if !roots.iter().any(|root| root.path == default_root.path) {
            roots.insert(0, default_root);
        }
        tracing::debug!(
            target: STORE_TRACE_TARGET,
            reindex,
            configured_roots = roots.len(),
            "ticket_store_scan_roots_loaded"
        );

        let mut integrated = 0usize;
        let mut diagnostics = Vec::new();
        let mut disk_ids = HashSet::new();
        let mut workflow_root_ids = HashSet::new();

        for (index, root) in roots.iter().enumerate() {
            if !root.path.exists() {
                continue;
            }
            let root_label = metric_root_label(index, root);
            let _root_span_guard = tracing::debug_span!(
                target: STORE_TRACE_TARGET,
                "ticket_store_scan_root",
                root_label = %root_label,
            )
            .entered();
            let scan_root_started = Instant::now();
            let (entries, diags) = TicketFs::scan_root(&root.path)?;
            record_named_phase_timing(
                &mut phase_timings_ms,
                format!("scan_root_{root_label}_ms"),
                scan_root_started,
            );
            record_named_phase_timing(
                &mut phase_timings_ms,
                "integration.manifest_parse_ms".to_string(),
                scan_root_started,
            );
            root_entry_counts.insert(root_label.clone(), entries.len());
            tracing::debug!(
                target: STORE_TRACE_TARGET,
                entries = entries.len(),
                diagnostics = diags.len(),
                "ticket_store_scan_root_discovered"
            );
            diagnostics.extend(diags);

            let integrate_root_started = Instant::now();
            let mut search_documents = Vec::with_capacity(entries.len());
            let mut indexed_updates = Vec::with_capacity(entries.len());
            let mut edge_updates = Vec::new();
            for entry in entries {
                let entry_id = entry.id;
                disk_ids.insert(entry_id);
                if let Some(update) =
                    integrate_entry(&self.index, entry, reindex)?
                {
                    *phase_timings_ms
                        .entry("integration.description_read_ms".to_string())
                        .or_insert(0) += update.description_read_ms;
                    indexed_updates.push(update.indexed);
                    edge_updates.extend(update.edges);
                    search_documents.push(update.search_document);
                    workflow_root_ids.insert(entry_id);
                }
                integrated += 1;
            }

            let index_upsert_started = Instant::now();
            self.index.upsert_tickets_batch(&indexed_updates)?;
            add_phase_elapsed(
                &mut phase_timings_ms,
                "integration.index_upsert_ms",
                index_upsert_started,
            );

            let edge_write_started = Instant::now();
            self.index.insert_edges_batch(&edge_updates)?;
            add_phase_elapsed(
                &mut phase_timings_ms,
                "integration.edge_write_ms",
                edge_write_started,
            );

            let search_upsert_started = Instant::now();
            self.search.upsert_batch(&search_documents)?;
            add_phase_elapsed(
                &mut phase_timings_ms,
                "integration.search_upsert_ms",
                search_upsert_started,
            );
            record_named_phase_timing(
                &mut phase_timings_ms,
                format!("integrate_root_{root_label}_ms"),
                integrate_root_started,
            );
            tracing::debug!(
                target: STORE_TRACE_TARGET,
                integrated,
                "ticket_store_scan_root_integrated"
            );
        }

        let mut pruned = 0usize;
        let mut stale_ids = Vec::new();
        let prune_started = Instant::now();
        for ticket in self.index.list_tickets()? {
            if !disk_ids.contains(&ticket.id) {
                diagnostics
                    .push(stale_reconciliation_diagnostic(&ticket, &roots));
                stale_ids.push(ticket.id);
                workflow_root_ids.insert(ticket.id);
                pruned += 1;
            }
        }
        self.index.remove_tickets_batch(&stale_ids)?;
        self.search.remove_batch(&stale_ids)?;
        record_phase_timing(
            &mut phase_timings_ms,
            "prune_stale_ms",
            prune_started,
        );

        let workflow_started = Instant::now();
        let workflow_timings = if reindex {
            self.rebuild_workflow_facts()?
        } else {
            let mut roots = workflow_root_ids.into_iter().collect::<Vec<_>>();
            roots.sort_unstable();
            self.refresh_workflow_facts_for_roots_with_timings(
                &roots,
                false,
                Utc::now(),
            )?
        };
        merge_phase_totals(&mut phase_timings_ms, workflow_timings);
        record_phase_timing(
            &mut phase_timings_ms,
            "rebuild_workflow_facts_ms",
            workflow_started,
        );

        tracing::debug!(
            target: STORE_TRACE_TARGET,
            integrated,
            pruned,
            diagnostics = diagnostics.len(),
            "ticket_store_scan_once_complete"
        );

        Ok(ScanReport {
            integrated,
            pruned,
            diagnostics,
            phase_timings_ms,
            root_entry_counts,
            skipped_roots,
        })
    }

    fn backfill_file_backed_edges_from_index(
        &self
    ) -> Result<(), StorageError> {
        for edge in self.index.list_all_edges()? {
            if !is_file_backed_edge_kind(&edge.kind) {
                continue;
            }

            let Some(source) = self.get_indexed(&edge.from)? else {
                continue;
            };
            if !source.path.join(TICKET_MANIFEST_FILE).is_file() {
                continue;
            }

            TicketFs::update_edge_field(
                &source.path,
                &edge.kind,
                edge.to,
                true,
            )?;
        }

        Ok(())
    }

    pub fn integrate_orphan(
        &self,
        path: &Path,
    ) -> Result<bool, StorageError> {
        let id: Uuid = match path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse().ok())
        {
            Some(id) => id,
            None => return Ok(false),
        };

        let manifest = match TicketFs::read(path) {
            Ok(manifest) => manifest,
            Err(_) => return Ok(false),
        };

        let entry = TicketScanEntry {
            id,
            path: path.to_path_buf(),
            manifest,
        };
        let update = integrate_entry(&self.index, entry, true)?;
        if let Some(update) = update {
            self.index.upsert_tickets_batch(&[update.indexed])?;
            self.index.insert_edges_batch(&update.edges)?;
            self.search.upsert_batch(&[update.search_document])?;
        }
        self.refresh_workflow_facts_for_roots(&[id], false, Utc::now())?;
        Ok(true)
    }
}

#[path = "scan_helpers.rs"]
mod scan_helpers;
use scan_helpers::*;
