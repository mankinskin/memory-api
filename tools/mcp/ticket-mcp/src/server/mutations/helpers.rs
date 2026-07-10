use std::{
    collections::BTreeMap,
    path::PathBuf,
};

use serde_json::Value;
use ticket_api::model::{
    edge::EdgeRecord,
    ticket::TicketManifest,
};
use uuid::Uuid;

use super::{
    McpError,
    TicketDetail,
    TicketServer,
};

pub(super) fn move_plan_json(
    report: &ticket_api::storage::move_planner::MovePreflightReport
) -> Result<Value, McpError> {
    Ok(serde_json::json!({
        "supported": report.supported(),
        "source_workspace_root": normalize_display_path(&report.source_workspace_root)?,
        "target_workspace_root": normalize_display_path(&report.target_workspace_root)?,
        "source_store_root": normalize_display_path(&report.source_store_root)?,
        "target_store_root": normalize_display_path(&report.target_store_root)?,
        "source_ticket_path": normalize_display_path(&report.source_entity_path)?,
        "destination_ticket_path": normalize_display_path(&report.destination_entity_path)?,
        "path_reference_files": report.path_reference_files
            .iter()
            .map(|path| normalize_display_path(path))
            .collect::<Result<Vec<_>, _>>()?,
        "reference_visibility": report.reference_visibility,
        "active_board_entries": report.active_board_entries,
        "historical_board_entries": report.historical_board_entries,
        "active_leases": report.active_leases,
        "blockers": report.blockers,
        "captured_at": report.captured_at,
    }))
}

pub(super) fn move_outcome_json(
    outcome: &ticket_api::storage::move_execution::MoveExecutionOutcome
) -> Value {
    serde_json::json!({
        "resumed": outcome.resumed,
        "rolled_back": outcome.rolled_back,
        "journal": {
            "id": outcome.journal.id,
            "ticket_id": outcome.journal.entity_id,
            "phase": outcome.journal.phase,
            "steps": outcome.journal.steps,
            "rollback_steps": outcome.journal.rollback_steps,
            "failure": outcome.journal.failure,
            "next_recovery_step": outcome.journal.next_recovery_step,
            "rewritten_path_files": outcome.journal.rewritten_path_files,
            "manual_followups": outcome.journal.manual_followups,
            "migrated_board_entries": outcome.journal.migrated_board_entries,
            "created_at": outcome.journal.created_at,
            "updated_at": outcome.journal.updated_at,
        }
    })
}

pub(super) fn move_recovery_json() -> Value {
    serde_json::json!({
        "resume": "move_resume { workspace, id: <journal-uuid> }",
        "rollback": "move_rollback { workspace, id: <journal-uuid> }",
    })
}

pub(super) fn normalize_workspace_root(
    value: &str
) -> Result<PathBuf, McpError> {
    ticket_api::workspace::canonicalize_workspace_root_strict(
        std::path::Path::new(value),
    )
    .map_err(|error| {
        McpError::invalid_params(
            format!(
                "workspace root canonicalization failed for '{}': {error}",
                value
            ),
            None,
        )
    })
}

pub(super) fn normalize_display_path(
    path: &std::path::Path
) -> Result<String, McpError> {
    ticket_api::workspace::normalize_path_for_display_strict(path).map_err(
        |error| {
            McpError::invalid_params(
                format!(
                    "path payload normalization failed for '{}': {error}",
                    path.display()
                ),
                None,
            )
        },
    )
}

pub(super) fn parse_field_patch(
    fields: Option<Vec<String>>,
    field_map: Option<BTreeMap<String, Value>>,
) -> Result<BTreeMap<String, Value>, McpError> {
    let mut patch = field_map.unwrap_or_default();

    for raw in fields.unwrap_or_default() {
        let (key, value) = raw.split_once('=').ok_or_else(|| {
            McpError::invalid_params(
                format!("invalid field format '{raw}', expected key=value"),
                None,
            )
        })?;
        patch.insert(
            key.trim().to_string(),
            Value::String(value.trim().to_string()),
        );
    }

    Ok(patch)
}

pub(super) fn resolve_edge_for_remove(
    from_selector: &str,
    to_selector: &str,
    kind: &str,
    store: &ticket_api::storage::TicketStore,
) -> Result<EdgeRecord, McpError> {
    let from = resolve_from_selector_for_remove(from_selector, store)?;
    let to = resolve_to_selector_for_remove(from, to_selector, kind, store)?;
    Ok(EdgeRecord {
        from,
        to,
        kind: kind.to_string(),
        created_at: chrono::Utc::now(),
    })
}

fn resolve_from_selector_for_remove(
    selector: &str,
    store: &ticket_api::storage::TicketStore,
) -> Result<Uuid, McpError> {
    let trimmed = selector.trim();
    if let Ok(id) = trimmed.parse::<Uuid>() {
        return Ok(id);
    }
    if trimmed.len() >= 8 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return TicketServer::resolve_uuid_with(store, trimmed).map_err(|_| {
            McpError::invalid_params(
                format!(
                    "cannot resolve source prefix '{trimmed}'; provide full UUID when source ticket is missing"
                ),
                None,
            )
        });
    }

    Err(McpError::invalid_params(
        format!(
            "invalid UUID '{selector}': expected full UUID or hex prefix (>= 8 chars)"
        ),
        None,
    ))
}

fn resolve_to_selector_for_remove(
    from: Uuid,
    selector: &str,
    kind: &str,
    store: &ticket_api::storage::TicketStore,
) -> Result<Uuid, McpError> {
    let trimmed = selector.trim();
    if let Ok(id) = trimmed.parse::<Uuid>() {
        return Ok(id);
    }
    if !(trimmed.len() >= 8 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit()))
    {
        return Err(McpError::invalid_params(
            format!(
                "invalid UUID '{selector}': expected full UUID or hex prefix (>= 8 chars)"
            ),
            None,
        ));
    }

    let prefix = trimmed.to_ascii_lowercase();
    let matches: Vec<Uuid> = store
        .edges_from(&from)
        .map_err(TicketServer::store_err)?
        .into_iter()
        .filter(|edge| edge.kind == kind)
        .map(|edge| edge.to)
        .filter(|to| to.simple().to_string().starts_with(&prefix))
        .collect();

    match matches.len() {
        0 => Err(McpError::invalid_params(
            format!(
                "edge not found: kind='{kind}' from='{from}' to='{selector}'"
            ),
            None,
        )),
        1 => Ok(matches[0]),
        count => Err(McpError::invalid_params(
            format!(
                "ambiguous target selector '{selector}' for from='{from}' kind='{kind}' (matches {count}); use full UUID"
            ),
            None,
        )),
    }
}

pub(super) fn indexed_ticket_path(
    store: &ticket_api::storage::store::TicketStore,
    id: &uuid::Uuid,
) -> Result<Option<String>, McpError> {
    Ok(store
        .get_indexed(id)
        .map_err(TicketServer::store_err)?
        .map(|ticket| ticket.path.display().to_string()))
}

pub(super) fn detail_from_manifest(
    manifest: TicketManifest,
    path: Option<String>,
) -> TicketDetail {
    TicketDetail {
        id: manifest.id.to_string(),
        path,
        created_at: manifest.created_at,
        fields: manifest.extra,
    }
}
