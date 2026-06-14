//! Ticket store index generator (ticket `c5e9bb39`).
//!
//! Reads all tickets from a `TicketStore`-compatible entity store
//! (exposed via [`EntityStore::list_indexed`]) and produces an [`IndexSidecar`]
//! for `.ticket/index.toon`.
//!
//! Full ticket manifests are NOT read here; the generator works entirely from
//! the indexed metadata (`IndexedEntity`) plus the optional description file.
//! This keeps generation fast enough for a pre-commit hook (D2).

use std::path::Path;

use chrono::Utc;
use uuid::Uuid;

use crate::model::index_entry::{
    ContentKind,
    IndexEntry,
    IndexRelations,
};
use crate::model::index_sidecar::IndexSidecar;
use crate::storage::indexed::IndexedEntity;

/// Input for the ticket sidecar generator.
pub struct TicketIndexInput<'a> {
    /// All indexed ticket entities to include.
    pub tickets: &'a [IndexedEntity],
    /// Workspace root used to compute workspace-relative `source_path` values.
    pub workspace_root: &'a Path,
    /// Store folder relative to workspace root (default: `.ticket`).
    pub store_dir: &'a str,
}

/// Generate an [`IndexSidecar`] for the ticket store.
///
/// Each `IndexedEntity` in `input.tickets` becomes one [`IndexEntry`] with:
/// - `kind` = [`ContentKind::Ticket`]
/// - `source_path` = workspace-relative path to the ticket folder (using `/` separators)
/// - `title` / `summary` drawn from the indexed fields
/// - `tags` include the ticket state if present
///
/// The returned sidecar is sorted by id and all entries are sealed.
pub fn generate_ticket_sidecar(input: TicketIndexInput<'_>) -> IndexSidecar {
    let mut entries: Vec<IndexEntry> = input
        .tickets
        .iter()
        .map(|t| make_ticket_entry(t, input.workspace_root))
        .collect();

    for e in &mut entries {
        e.seal();
    }

    let mut sidecar = IndexSidecar::new(
        ContentKind::Ticket,
        input.store_dir,
        entries,
    );
    sidecar.sort();
    sidecar
}

fn make_ticket_entry(
    t: &IndexedEntity,
    workspace_root: &Path,
) -> IndexEntry {
    let source_path = to_relative_slash(workspace_root, &t.path);
    let title = t.title.clone().unwrap_or_else(|| t.id.to_string());
    let state = t.state.clone().unwrap_or_default();

    let tags = if state.is_empty() {
        vec![]
    } else {
        vec![state.clone()]
    };

    let keywords = {
        let mut kw: Vec<String> = title
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .map(|w| w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|w| !w.is_empty())
            .collect();
        kw.sort_unstable();
        kw.dedup();
        kw
    };

    IndexEntry {
        id: t.id,
        kind: ContentKind::Ticket,
        source_path,
        title,
        summary: state.clone(),
        keywords,
        scope: None,
        non_goals: None,
        relations: IndexRelations::default(),
        digest: String::new(),
        tags,
        generated_at: Utc::now(),
        source_modified_at: Some(t.updated_at),
    }
}

/// Convert an absolute path to a workspace-relative string with `/` separators.
pub(super) fn to_relative_slash(
    workspace_root: &Path,
    abs_path: &Path,
) -> String {
    abs_path
        .strip_prefix(workspace_root)
        .unwrap_or(abs_path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Stable synthetic UUID derived from a fixed namespace + slug string.
///
/// Used for workspace summary and agent-hook entries that have no store UUID.
pub(super) fn deterministic_uuid(namespace: Uuid, slug: &str) -> Uuid {
    Uuid::new_v5(&namespace, slug.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use chrono::Utc;

    fn fake_ticket(id: Uuid, title: &str, state: &str, path: PathBuf) -> IndexedEntity {
        IndexedEntity {
            id,
            path,
            type_id: "ticket".to_string(),
            title: Some(title.to_string()),
            state: Some(state.to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn generate_ticket_sidecar_produces_sealed_sorted_entries() {
        let ws = PathBuf::from("/workspace");
        let id_a = Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000000").unwrap();
        let id_b = Uuid::parse_str("bbbbbbbb-0000-0000-0000-000000000000").unwrap();

        let tickets = vec![
            fake_ticket(id_b, "Ticket B", "done", ws.join(".ticket/tickets/b/ticket.toml")),
            fake_ticket(id_a, "Ticket A", "new", ws.join(".ticket/tickets/a/ticket.toml")),
        ];

        let sidecar = generate_ticket_sidecar(TicketIndexInput {
            tickets: &tickets,
            workspace_root: &ws,
            store_dir: ".ticket",
        });

        assert_eq!(sidecar.entries.len(), 2);
        // sorted by id ascending
        assert_eq!(sidecar.entries[0].id, id_a);
        assert_eq!(sidecar.entries[1].id, id_b);
        // all sealed
        for e in &sidecar.entries {
            assert!(!e.digest.is_empty(), "entry should be sealed");
            assert!(e.is_digest_valid(), "sealed digest should be valid");
        }
        // source_path uses forward slashes and is relative
        assert!(sidecar.entries[0].source_path.starts_with(".ticket/"));
        assert!(!sidecar.entries[0].source_path.contains('\\'));
    }

}
