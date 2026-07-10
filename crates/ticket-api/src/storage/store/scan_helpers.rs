use super::*;

pub(super) fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

pub(super) fn record_phase_timing(
    timings: &mut std::collections::BTreeMap<String, u64>,
    phase: &'static str,
    started: Instant,
) {
    record_named_phase_timing(timings, phase.to_string(), started);
}

pub(super) fn record_named_phase_timing(
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

pub(super) fn metric_root_label(
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

pub(super) fn sanitize_metric_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

pub(super) fn stale_reconciliation_diagnostic(
    ticket: &IndexedTicket,
    roots: &[ScanRoot],
) -> ParseDiagnostic {
    let manifest_path = ticket.path.join(TICKET_MANIFEST_FILE);
    let reason = if roots
        .iter()
        .all(|root| !ticket.path.starts_with(&root.path))
    {
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

pub(super) fn integrate_entry(
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
    let effort_str = entry.manifest.extra.get("effort").and_then(|v| match v {
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

pub(super) struct ScanIntegrationUpdate {
    pub(super) indexed: IndexedTicket,
    pub(super) edges: Vec<EdgeRecord>,
    pub(super) search_document: SearchDocumentInput,
    pub(super) description_read_ms: u64,
}

pub(super) fn add_phase_elapsed(
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

pub(super) fn entry_is_current(
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
    if path_modified_after(&entry.path.join(TICKET_MANIFEST_FILE), indexed_at)?
    {
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

pub(super) fn path_modified_after(
    path: &Path,
    indexed_at: chrono::DateTime<Utc>,
) -> Result<bool, StorageError> {
    let modified = fs::metadata(path)?.modified()?;
    let modified_at = chrono::DateTime::<Utc>::from(modified);
    Ok(modified_at > indexed_at)
}

pub(super) fn merge_phase_totals(
    timings: &mut std::collections::BTreeMap<String, u64>,
    phase_totals: std::collections::BTreeMap<String, u64>,
) {
    for (phase, elapsed) in phase_totals {
        *timings.entry(phase).or_insert(0) += elapsed;
    }
}

pub(super) fn manifest_edges(
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

pub(super) fn resolve_known_ticket_entry(
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

pub(super) fn is_file_backed_edge_kind(kind: &str) -> bool {
    FILE_BACKED_EDGE_KINDS.contains(&kind)
}
