mod board;
mod crud;
mod edges;
mod history;
mod lifecycle;
mod ops;
mod parts;
mod query;
mod workspace;

pub(crate) use board::*;
pub(crate) use crud::*;
pub(crate) use edges::*;
pub(crate) use history::*;
pub(crate) use lifecycle::*;
pub(crate) use ops::*;
pub(crate) use parts::*;
pub(crate) use query::*;
pub(crate) use workspace::*;

use crate::cli::CliRunError;
use serde_json::{
    Value,
    json,
};
use ticket_api::storage::TicketStore;
use uuid::Uuid;

fn normalize_display_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn ticket_workspace_metadata_for_path(
    store: &TicketStore,
    ticket_path: &std::path::Path,
) -> Value {
    let active_index_root = store.index_root.clone();
    let store_root = ticket_api::workspace::resolve_store_root_from(
        ticket_path,
        ticket_api::workspace::TICKET_INDEX_DIR,
    );
    let workspace_root =
        ticket_api::workspace::resolve_workspace_root_from_store_root(
            &store_root,
            ticket_api::workspace::TICKET_INDEX_DIR,
        );

    json!({
        "active_index_root": normalize_display_path(&active_index_root),
        "store_root": normalize_display_path(&store_root),
        "workspace_root": normalize_display_path(&workspace_root),
    })
}

pub(crate) fn ticket_workspace_metadata_for_id(
    store: &TicketStore,
    ticket_id: Uuid,
) -> Option<Value> {
    store
        .get_indexed(&ticket_id)
        .ok()
        .flatten()
        .map(|indexed| ticket_workspace_metadata_for_path(store, &indexed.path))
}

fn workspace_recovery_hint(store: &TicketStore) -> String {
    ticket_api::workspace::workspace_recovery_hint(&store.index_root)
}

/// Resolve a UUID string that may be a full UUID or a hex prefix (>= 8 chars).
pub(crate) fn resolve_uuid_prefix(
    s: &str,
    store: &TicketStore,
) -> Result<Uuid, CliRunError> {
    if let Ok(id) = s.parse::<Uuid>() {
        return Ok(id);
    }

    let trimmed = s.trim();
    if trimmed.len() >= 8 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        let tickets = store.list(None, None, None)?;
        let prefix_lower = trimmed.to_ascii_lowercase();
        let matches: Vec<Uuid> = tickets
            .iter()
            .filter(|t| t.id.simple().to_string().starts_with(&prefix_lower))
            .map(|t| t.id)
            .collect();

        return match matches.len() {
            1 => Ok(matches[0]),
            0 => Err(CliRunError::BadRequest(format!(
                "no ticket found matching prefix '{trimmed}'; {}",
                workspace_recovery_hint(store)
            ))),
            n => Err(CliRunError::BadRequest(format!(
                "ambiguous prefix '{trimmed}': matches {n} tickets"
            ))),
        };
    }

    Err(CliRunError::BadRequest(format!(
        "invalid UUID '{s}': expected full UUID or hex prefix (>= 8 chars)"
    )))
}
