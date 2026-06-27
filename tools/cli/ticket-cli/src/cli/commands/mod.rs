mod board;
mod crud;
mod edges;
mod history;
mod lifecycle;
mod ops;
mod query;

pub(crate) use board::*;
pub(crate) use crud::*;
pub(crate) use edges::*;
pub(crate) use history::*;
pub(crate) use lifecycle::*;
pub(crate) use ops::*;
pub(crate) use query::*;

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
    let workspace_root =
        ticket_api::workspace::resolve_workspace_root_from_store_root(
            &store.index_root,
            ticket_api::workspace::TICKET_INDEX_DIR,
        );
    let discovered = ticket_api::workspace::find_descendant_store_roots_from(
        &workspace_root,
        ticket_api::workspace::TICKET_INDEX_DIR,
    );
    let discovered = discovered
        .into_iter()
        .map(|path| normalize_display_path(&path))
        .collect::<Vec<_>>();

    if discovered.is_empty() {
        return format!(
            "active index root: {}. Retry with --workspace-root <workspace-path> or --index-root <path-to-.ticket>",
            normalize_display_path(&store.index_root)
        );
    }

    format!(
        "active index root: {}. Retry with --workspace-root <workspace-path> or --index-root <path-to-.ticket>. Discovered ticket stores: {}",
        normalize_display_path(&store.index_root),
        discovered.join(", ")
    )
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
