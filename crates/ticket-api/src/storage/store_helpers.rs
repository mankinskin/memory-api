use super::*;

pub(super) fn strip_file_backed_edge_fields(
    patch: &mut BTreeMap<String, Value>,
) {
    for field in FILE_BACKED_EDGE_FIELDS {
        patch.remove(*field);
    }
}

pub(super) fn edge_patch_plans(
    patch: &BTreeMap<String, Value>,
    current_extra: &BTreeMap<String, Value>,
) -> Result<Vec<EdgePatchPlan>, StorageError> {
    let mut plans = Vec::new();

    for field in FILE_BACKED_EDGE_FIELDS {
        let Some(requested_value) = patch.get(*field) else {
            continue;
        };

        let desired = parse_requested_edge_targets(requested_value, field)?;
        let current = parse_manifest_edge_targets(current_extra.get(*field), field)?;

        let to_add = desired
            .difference(&current)
            .copied()
            .collect::<Vec<_>>();
        let to_remove = current
            .difference(&desired)
            .copied()
            .collect::<Vec<_>>();

        plans.push(EdgePatchPlan {
            kind: (*field).to_string(),
            to_add,
            to_remove,
        });
    }

    Ok(plans)
}

pub(super) fn parse_manifest_edge_targets(
    value: Option<&Value>,
    edge_kind: &str,
) -> Result<BTreeSet<Uuid>, StorageError> {
    match value {
        None => Ok(BTreeSet::new()),
        Some(value) => parse_requested_edge_targets(value, edge_kind),
    }
}

pub(super) fn parse_requested_edge_targets(
    value: &Value,
    edge_kind: &str,
) -> Result<BTreeSet<Uuid>, StorageError> {
    match value {
        Value::Array(items) => parse_edge_target_array(items, edge_kind),
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(BTreeSet::new());
            }
            if trimmed.starts_with('[') {
                let parsed: Value = serde_json::from_str(trimmed).map_err(|e| {
                    StorageError::Other(format!(
                        "edge field '{}' string payload must be a JSON array or single UUID: {e}",
                        edge_kind
                    ))
                })?;
                let Value::Array(items) = parsed else {
                    return Err(StorageError::Other(format!(
                        "edge field '{}' JSON payload must be an array",
                        edge_kind
                    )));
                };
                parse_edge_target_array(&items, edge_kind)
            } else {
                let id = Uuid::parse_str(trimmed).map_err(|e| {
                    StorageError::Other(format!(
                        "edge field '{}' contains invalid ticket id '{}': {e}",
                        edge_kind, trimmed
                    ))
                })?;
                Ok(BTreeSet::from([id]))
            }
        }
        _ => Err(StorageError::Other(format!(
            "edge field '{}' must be an array, JSON array string, or UUID string",
            edge_kind
        ))),
    }
}

pub(super) fn parse_edge_target_array(
    items: &[Value],
    edge_kind: &str,
) -> Result<BTreeSet<Uuid>, StorageError> {
    let mut set = BTreeSet::new();
    for item in items {
        let Some(id_text) = item.as_str() else {
            return Err(StorageError::Other(format!(
                "edge field '{}' must contain only string ticket IDs",
                edge_kind
            )));
        };
        let id = Uuid::parse_str(id_text).map_err(|e| {
            StorageError::Other(format!(
                "edge field '{}' contains invalid ticket id '{}': {e}",
                edge_kind, id_text
            ))
        })?;
        set.insert(id);
    }
    Ok(set)
}

pub(super) fn apply_edge_patch_plans(
    store: &TicketStore,
    from: Uuid,
    plans: Vec<EdgePatchPlan>,
) -> Result<(), StorageError> {
    for plan in plans {
        for to in plan.to_remove {
            store.remove_edge(crate::model::edge::EdgeRecord {
                from,
                to,
                kind: plan.kind.clone(),
                created_at: Utc::now(),
            })?;
        }
        for to in plan.to_add {
            store.add_edge(crate::model::edge::EdgeRecord {
                from,
                to,
                kind: plan.kind.clone(),
                created_at: Utc::now(),
            })?;
        }
    }
    Ok(())
}

pub(super) fn emit_store_open_report(
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

pub(super) fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

pub(super) fn merge_timings(
    target: &mut BTreeMap<String, u64>,
    source: BTreeMap<String, u64>,
) {
    target.extend(source);
}

pub(super) fn merge_prefixed_timings(
    target: &mut BTreeMap<String, u64>,
    prefix: &str,
    source: BTreeMap<String, u64>,
) {
    for (key, value) in source {
        target.insert(format!("{prefix}_{key}"), value);
    }
}

pub(super) fn scan_root_has_ticket_manifests(root: &Path) -> Result<bool, StorageError> {
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

