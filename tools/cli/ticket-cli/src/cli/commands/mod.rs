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

use uuid::Uuid;
use ticket_api::storage::TicketStore;
use crate::cli::CliRunError;

/// Resolve a UUID string that may be a full UUID or a hex prefix (>= 8 chars).
pub(crate) fn resolve_uuid_prefix(s: &str, store: &TicketStore) -> Result<Uuid, CliRunError> {
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
                "no ticket found matching prefix '{trimmed}'"
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
