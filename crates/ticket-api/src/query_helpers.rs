use uuid::Uuid;

use crate::{
    error::StorageError,
    storage::{
        indexed::IndexedTicket,
        store::TicketStore,
        ticket_fs::TicketFs,
    },
};

pub fn parse_where_filters(
    clauses: &[String]
) -> Result<Vec<(String, String)>, String> {
    let mut filters = Vec::new();
    for clause in clauses {
        let Some((key, value)) = clause.split_once('=') else {
            return Err("where clauses must be key=value".to_string());
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return Err("where clauses must be key=value".to_string());
        }
        filters.push((key.to_string(), value.to_string()));
    }
    Ok(filters)
}

pub fn apply_field_filters(
    tickets: Vec<IndexedTicket>,
    filters: &[(String, String)],
) -> Vec<IndexedTicket> {
    if filters.is_empty() {
        return tickets;
    }

    tickets
        .into_iter()
        .filter(|ticket| ticket_matches_field_filters(ticket, filters))
        .collect()
}

pub fn resolve_uuid_with_prefix(
    store: &TicketStore,
    input: &str,
) -> Result<Uuid, StorageError> {
    if let Ok(uuid) = Uuid::parse_str(input) {
        return Ok(uuid);
    }

    let prefix = input.trim();
    if prefix.len() < 8 || !prefix.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(StorageError::Other(format!(
            "invalid UUID '{input}': expected full UUID or hex prefix (>= 8 chars)",
        )));
    }

    let prefix_lower = prefix.to_ascii_lowercase();
    let matches: Vec<Uuid> = store
        .list(None, None, None)?
        .into_iter()
        .map(|ticket| ticket.id)
        .filter(|id| id.simple().to_string().starts_with(&prefix_lower))
        .collect();

    match matches.as_slice() {
        [single] => Ok(*single),
        [] => Err(StorageError::Other(format!(
            "no ticket found matching prefix '{prefix}'",
        ))),
        _ => Err(StorageError::Other(format!(
            "ambiguous prefix '{prefix}': matches {} tickets",
            matches.len()
        ))),
    }
}

fn ticket_matches_field_filters(
    ticket: &IndexedTicket,
    filters: &[(String, String)],
) -> bool {
    let needs_manifest =
        filters.iter().any(|(key, _)| !is_indexed_ticket_field(key));
    let manifest = if needs_manifest {
        TicketFs::read(&ticket.path).ok()
    } else {
        None
    };

    filters.iter().all(|(key, expected)| {
        indexed_ticket_field(ticket, key).or_else(|| {
            manifest.as_ref().and_then(|entity| {
                entity.extra.get(key).and_then(|value| value.as_str())
            })
        }) == Some(expected.as_str())
    })
}

fn is_indexed_ticket_field(key: &str) -> bool {
    matches!(key, "state" | "type" | "title")
}

fn indexed_ticket_field<'a>(
    ticket: &'a IndexedTicket,
    key: &str,
) -> Option<&'a str> {
    match key {
        "state" => ticket.state.as_deref(),
        "type" => Some(ticket.type_id.as_str()),
        "title" => ticket.title.as_deref(),
        _ => None,
    }
}
