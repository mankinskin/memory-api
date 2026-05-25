use std::{
    collections::HashSet,
    path::Path,
};

use chrono::Utc;
use uuid::Uuid;

use crate::{
    error::StorageError,
    model::filesystem::{
        ParseDiagnostic,
        ScanRoot,
        TICKET_MANIFEST_FILE,
    },
    storage::{
        index::RedbIndexStore,
        indexed::IndexedTicket,
        search::TantivySearchIndex,
        ticket_fs::{
            TicketFs,
            TicketScanEntry,
        },
    },
};

use super::TicketStore;

const FILE_BACKED_EDGE_KINDS: &[&str] = &["depends_on", "linked"];

pub struct ScanReport {
    pub integrated: usize,
    pub pruned: usize,
    pub diagnostics: Vec<ParseDiagnostic>,
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
        if reindex {
            self.backfill_file_backed_edges_from_index()?;
            self.search.clear_all()?;
            self.index.clear_edges()?;
        }

        let mut roots = self.list_scan_roots()?;
        let default_root = ScanRoot {
            path: self.resolve_scan_root_path(&self.index_root.join("tickets")),
            label: "default".to_string(),
        };
        if !roots.iter().any(|root| root.path == default_root.path) {
            roots.insert(0, default_root);
        }

        let mut integrated = 0usize;
        let mut diagnostics = Vec::new();
        let mut disk_ids = HashSet::new();

        for root in &roots {
            if !root.path.exists() {
                continue;
            }
            let (entries, diags) = TicketFs::scan_root(&root.path)?;
            diagnostics.extend(diags);

            for entry in entries {
                disk_ids.insert(entry.id);
                integrate_entry(&self.index, &self.search, entry, reindex)?;
                integrated += 1;
            }
        }

        let mut pruned = 0usize;
        for ticket in self.index.list_tickets(true)? {
            if !disk_ids.contains(&ticket.id) {
                diagnostics.push(stale_reconciliation_diagnostic(
                    &ticket,
                    &roots,
                ));
                self.index.remove_ticket(&ticket.id)?;
                self.search.remove(&ticket.id)?;
                pruned += 1;
            }
        }

        self.rebuild_workflow_facts()?;

        Ok(ScanReport {
            integrated,
            pruned,
            diagnostics,
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
            if source.deleted {
                continue;
            }
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
        let is_deleted = manifest
            .extra
            .get("deleted")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if is_deleted {
            return Ok(false);
        }

        let entry = TicketScanEntry {
            id,
            path: path.to_path_buf(),
            manifest,
        };
        integrate_entry(&self.index, &self.search, entry, true)?;
        self.refresh_workflow_facts_for_roots(&[id], false, Utc::now())?;
        Ok(true)
    }
}

fn stale_reconciliation_diagnostic(
    ticket: &IndexedTicket,
    roots: &[ScanRoot],
) -> ParseDiagnostic {
    let manifest_path = ticket.path.join(TICKET_MANIFEST_FILE);
    let reason = if roots.iter().all(|root| !ticket.path.starts_with(&root.path))
    {
        "ticket path left configured scan roots; pruned stale index/search entry"
            .to_string()
    } else if TicketFs::read(&ticket.path)
        .ok()
        .and_then(|manifest| {
            manifest
                .extra
                .get("deleted")
                .and_then(|value| value.as_bool())
        })
        == Some(true)
    {
        "ticket marked deleted on disk; pruned stale index/search entry"
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
    search: &TantivySearchIndex,
    entry: TicketScanEntry,
    _reindex: bool,
) -> Result<(), StorageError> {
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
            existing.path = entry.path.clone();
            existing.type_id = type_id.clone();
            existing.created_at = entry.manifest.created_at;
            existing.updated_at = now;
            existing.title = title.clone();
            existing.state = state.clone();
            existing.deleted = false;
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
            deleted: false,
        },
    };
    index.insert_ticket(&indexed)?;
    for edge in manifest_edges(&entry) {
        index.insert_edge(&edge)?;
    }

    let body = TicketFs::read_description(&entry.path);
    search.upsert(
        &entry.id,
        title.as_deref(),
        body.as_deref(),
        state.as_deref(),
        Some(&type_id),
    )?;

    Ok(())
}

fn manifest_edges(
    entry: &TicketScanEntry
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

fn is_file_backed_edge_kind(kind: &str) -> bool {
    FILE_BACKED_EDGE_KINDS.contains(&kind)
}
