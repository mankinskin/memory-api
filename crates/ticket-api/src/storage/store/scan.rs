use std::collections::HashSet;
use std::path::Path;

use chrono::Utc;
use uuid::Uuid;

use crate::error::StorageError;
use crate::model::filesystem::{ParseDiagnostic, ScanRoot};
use crate::storage::index::RedbIndexStore;
use crate::storage::indexed::IndexedTicket;
use crate::storage::search::TantivySearchIndex;
use crate::storage::ticket_fs::{TicketFs, TicketScanEntry};

use super::TicketStore;

pub struct ScanReport {
    pub integrated: usize,
    pub pruned: usize,
    pub diagnostics: Vec<ParseDiagnostic>,
}

impl TicketStore {
    pub fn add_scan_root(&self, root: ScanRoot) -> Result<(), StorageError> {
        self.index.add_scan_root(&root)
    }

    pub fn list_scan_roots(&self) -> Result<Vec<ScanRoot>, StorageError> {
        self.index.list_scan_roots()
    }

    pub fn scan(&self, reindex: bool) -> Result<ScanReport, StorageError> {
        if reindex {
            self.search.clear_all()?;
        }

        let roots = self.index.list_scan_roots()?;
        let default_root = ScanRoot {
            path: self.index_root.join("tickets"),
            label: "default".to_string(),
        };
        let all_roots: Vec<&ScanRoot> = std::iter::once(&default_root).chain(roots.iter()).collect();

        let mut integrated = 0usize;
        let mut diagnostics = Vec::new();
        let mut disk_ids = HashSet::new();

        for root in all_roots {
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
        if reindex {
            for ticket in self.index.list_tickets(true)? {
                if !disk_ids.contains(&ticket.id) {
                    self.index.remove_ticket(&ticket.id)?;
                    pruned += 1;
                }
            }
        }

        Ok(ScanReport {
            integrated,
            pruned,
            diagnostics,
        })
    }

    pub fn integrate_orphan(&self, path: &Path) -> Result<bool, StorageError> {
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
        Ok(true)
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