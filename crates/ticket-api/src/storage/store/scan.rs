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
        ScanRoot,
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
}

impl TicketStore {
    pub fn add_scan_root(
        &self,
        root: ScanRoot,
    ) -> Result<(), StorageError> {
        self.index.add_scan_root(&ScanRoot {
            path: self.resolve_scan_root_path(&root.path),
            label: root.label,
        })
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

    pub fn scan(
        &self,
        reindex: bool,
    ) -> Result<ScanReport, StorageError> {
        let span = tracing::info_span!(
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
        tracing::info!(
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
            let entry = resolve_known_ticket_entry(id, existing.as_ref(), &roots)?;
            match entry {
                Some(entry) => {
                    integrated += 1;
                    if let Some(update) = integrate_entry(&self.index, entry, false)? {
                        *phase_timings_ms
                            .entry("integration.description_read_ms".to_string())
                            .or_insert(0) += update.description_read_ms;
                        indexed_updates.push(update.indexed);
                        edge_updates.extend(update.edges);
                        search_documents.push(update.search_document);
                        workflow_root_ids.insert(id);
                    }
                },
                None => {
                    if let Some(ticket) = existing {
                        diagnostics.push(stale_reconciliation_diagnostic(&ticket, &roots));
                        stale_ids.push(id);
                        workflow_root_ids.insert(id);
                        pruned += 1;
                    }
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
        let workflow_timings = self.refresh_workflow_facts_for_roots_with_timings(
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
        let mut roots = self.list_scan_roots()?;
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
                if let Some(update) = integrate_entry(
                    &self.index,
                    entry,
                    reindex,
                )? {
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
                diagnostics.push(stale_reconciliation_diagnostic(
                    &ticket,
                    &roots,
                ));
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
        merge_phase_totals(
            &mut phase_timings_ms,
            workflow_timings,
        );
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
        })
    }

    fn backfill_file_backed_edges_from_index(
        &self,
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
        let update = integrate_entry(
            &self.index,
            entry,
            true,
        )?;
        if let Some(update) = update {
            self.index.upsert_tickets_batch(&[update.indexed])?;
            self.index.insert_edges_batch(&update.edges)?;
            self.search.upsert_batch(&[update.search_document])?;
        }
        self.refresh_workflow_facts_for_roots(&[id], false, Utc::now())?;
        Ok(true)
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

fn record_phase_timing(
    timings: &mut std::collections::BTreeMap<String, u64>,
    phase: &'static str,
    started: Instant,
) {
    record_named_phase_timing(timings, phase.to_string(), started);
}

fn record_named_phase_timing(
    timings: &mut std::collections::BTreeMap<String, u64>,
    phase: String,
    started: Instant,
) {
    let elapsed_ms = elapsed_ms(started);
    timings.insert(phase.clone(), elapsed_ms);
    tracing::debug!(
        target: STORE_TRACE_TARGET,
        phase = %phase,
        elapsed_ms,
        "ticket_store_phase_complete"
    );
}

fn metric_root_label(
    index: usize,
    root: &ScanRoot,
) -> String {
    let label = if root.label.trim().is_empty() {
        root.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("root")
    } else {
        root.label.as_str()
    };
    format!("{index}_{}", sanitize_metric_label(label))
}

fn sanitize_metric_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn stale_reconciliation_diagnostic(
    ticket: &IndexedTicket,
    roots: &[ScanRoot],
) -> ParseDiagnostic {
    let manifest_path = ticket.path.join(TICKET_MANIFEST_FILE);
    let reason = if roots.iter().all(|root| !ticket.path.starts_with(&root.path)) {
        "ticket path left configured scan roots; pruned stale index/search entry"
            .to_string()
    } else if !ticket.path.exists() {
        "ticket folder missing on disk; pruned stale index/search entry"
            .to_string()
    } else {
        "ticket missing from scan results; pruned stale index/search entry"
            .to_string()
    };

    ParseDiagnostic {
        path: manifest_path,
        reason,
    }
}

fn integrate_entry(
    index: &RedbIndexStore,
    entry: TicketScanEntry,
    reindex: bool,
) -> Result<Option<ScanIntegrationUpdate>, StorageError> {
    let type_id = entry
        .manifest
        .extra
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown")
        .to_string();
    let title = entry
        .manifest
        .extra
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let state = entry
        .manifest
        .extra
        .get("state")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let now = Utc::now();

    let indexed = match index.get_ticket(&entry.id)? {
        Some(mut existing) => {
            if !reindex && entry_is_current(&entry, &existing)? {
                return Ok(None);
            }
            existing.path = entry.path.clone();
            existing.type_id = type_id.clone();
            existing.created_at = entry.manifest.created_at;
            existing.updated_at = now;
            existing.title = title.clone();
            existing.state = state.clone();
            existing
        },
        None => IndexedTicket {
            id: entry.id,
            path: entry.path.clone(),
            type_id: type_id.clone(),
            title: title.clone(),
            state: state.clone(),
            created_at: entry.manifest.created_at,
            updated_at: now,
        },
    };
    let edges = manifest_edges(&entry);

    let description_read_started = Instant::now();
    let body = TicketFs::read_description(&entry.path);
    let description_read_ms = elapsed_ms(description_read_started);

    let created_at_str = indexed.created_at.to_rfc3339();
    let effort_str = entry
        .manifest
        .extra
        .get("effort")
        .and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        });
    Ok(Some(ScanIntegrationUpdate {
        indexed,
        edges,
        search_document: SearchDocumentInput {
            id: entry.id,
            title,
            body,
            state,
            ticket_type: Some(type_id),
            created_at: Some(created_at_str),
            effort: effort_str,
        },
        description_read_ms,
    }))
}

struct ScanIntegrationUpdate {
    indexed: IndexedTicket,
    edges: Vec<EdgeRecord>,
    search_document: SearchDocumentInput,
    description_read_ms: u64,
}

fn add_phase_elapsed(
    timings: &mut std::collections::BTreeMap<String, u64>,
    key: &str,
    started: Instant,
) {
    let elapsed = elapsed_ms(started);
    *timings.entry(key.to_string()).or_insert(0) += elapsed;
    tracing::debug!(
        target: STORE_TRACE_TARGET,
        phase = key,
        elapsed_ms = elapsed,
        "ticket_store_phase_complete"
    );
}

fn entry_is_current(
    entry: &TicketScanEntry,
    existing: &IndexedTicket,
) -> Result<bool, StorageError> {
    if existing.path != entry.path
        || existing.type_id
            != entry
                .manifest
                .extra
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
        || existing.title
            != entry
                .manifest
                .extra
                .get("title")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        || existing.state
            != entry
                .manifest
                .extra
                .get("state")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        || existing.created_at != entry.manifest.created_at
    {
        return Ok(false);
    }

    let indexed_at = existing.updated_at;
    if path_modified_after(&entry.path.join(TICKET_MANIFEST_FILE), indexed_at)? {
        return Ok(false);
    }

    let description_path = entry.path.join("description.md");
    if description_path.exists() {
        if path_modified_after(&description_path, indexed_at)? {
            return Ok(false);
        }
    } else if path_modified_after(&entry.path, indexed_at)? {
        return Ok(false);
    }

    Ok(true)
}

fn path_modified_after(
    path: &Path,
    indexed_at: chrono::DateTime<Utc>,
) -> Result<bool, StorageError> {
    let modified = fs::metadata(path)?.modified()?;
    let modified_at = chrono::DateTime::<Utc>::from(modified);
    Ok(modified_at > indexed_at)
}

fn merge_phase_totals(
    timings: &mut std::collections::BTreeMap<String, u64>,
    phase_totals: std::collections::BTreeMap<String, u64>,
) {
    for (phase, elapsed) in phase_totals {
        *timings.entry(phase).or_insert(0) += elapsed;
    }
}

fn manifest_edges(
    entry: &TicketScanEntry,
) -> Vec<crate::model::edge::EdgeRecord> {
    let mut edges = Vec::new();

    for &kind in FILE_BACKED_EDGE_KINDS {
        let Some(items) = entry
            .manifest
            .extra
            .get(kind)
            .and_then(|value| value.as_array())
        else {
            continue;
        };

        for item in items {
            let Some(target) = item.as_str() else {
                continue;
            };
            let Ok(to) = Uuid::parse_str(target) else {
                continue;
            };
            edges.push(crate::model::edge::EdgeRecord {
                from: entry.id,
                to,
                kind: kind.to_string(),
                created_at: entry.manifest.created_at,
            });
        }
    }

    edges
}

fn resolve_known_ticket_entry(
    id: Uuid,
    existing: Option<&IndexedTicket>,
    roots: &[ScanRoot],
) -> Result<Option<TicketScanEntry>, StorageError> {
    if let Some(existing) = existing {
        let manifest_path = existing.path.join(TICKET_MANIFEST_FILE);
        if manifest_path.is_file() {
            if let Ok(manifest) = TicketFs::read(&existing.path) {
                return Ok(Some(TicketScanEntry {
                    id,
                    path: existing.path.clone(),
                    manifest,
                }));
            }
        }
    }

    for root in roots {
        let candidate = root.path.join(id.to_string());
        if !candidate.join(TICKET_MANIFEST_FILE).is_file() {
            continue;
        }

        let manifest = TicketFs::read(&candidate)?;
        return Ok(Some(TicketScanEntry {
            id,
            path: candidate,
            manifest,
        }));
    }

    Ok(None)
}

fn is_file_backed_edge_kind(kind: &str) -> bool {
    FILE_BACKED_EDGE_KINDS.contains(&kind)
}
