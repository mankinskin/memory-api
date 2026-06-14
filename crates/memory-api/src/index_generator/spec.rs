//! Spec store index generator (ticket `b9757ba7`).
//!
//! Converts indexed spec entities into an [`IndexSidecar`] for `.spec/index.toon`.
//! Works from `IndexedEntity` metadata only (no full spec.toml read) so it stays
//! fast enough for a pre-commit hook. The full spec body/body-accessor is read
//! by the CLI `store-index` README generator (separate concern).

use std::path::Path;

use chrono::Utc;

use crate::model::index_entry::{
    ContentKind,
    IndexEntry,
    IndexRelations,
};
use crate::model::index_sidecar::IndexSidecar;
use crate::storage::indexed::IndexedEntity;

use super::ticket::to_relative_slash;

/// Input for the spec sidecar generator.
pub struct SpecIndexInput<'a> {
    /// All indexed spec entities.
    pub specs: &'a [IndexedEntity],
    /// Workspace root for relative path computation.
    pub workspace_root: &'a Path,
    /// Store folder relative to workspace root (default: `.spec`).
    pub store_dir: &'a str,
}

/// Generate an [`IndexSidecar`] for the spec store.
///
/// Each spec entity becomes one [`IndexEntry`] with:
/// - `kind` = [`ContentKind::Spec`]
/// - `source_path` = workspace-relative folder path (using `/` separators)
/// - `title` from indexed metadata, falling back to id string
/// - `tags` include the spec state if present
///
/// All entries are sealed and sorted by id.
pub fn generate_spec_sidecar(input: SpecIndexInput<'_>) -> IndexSidecar {
    let mut entries: Vec<IndexEntry> = input
        .specs
        .iter()
        .map(|s| make_spec_entry(s, input.workspace_root))
        .collect();

    for e in &mut entries {
        e.seal();
    }

    let mut sidecar = IndexSidecar::new(
        ContentKind::Spec,
        input.store_dir,
        entries,
    );
    sidecar.sort();
    sidecar
}

fn make_spec_entry(
    s: &IndexedEntity,
    workspace_root: &Path,
) -> IndexEntry {
    let source_path = to_relative_slash(workspace_root, &s.path);
    let title = s.title.clone().unwrap_or_else(|| s.id.to_string());
    let state = s.state.clone().unwrap_or_default();

    let tags = if state.is_empty() {
        vec![]
    } else {
        vec![state.clone()]
    };

    let keywords = extract_keywords(&title);

    IndexEntry {
        id: s.id,
        kind: ContentKind::Spec,
        source_path,
        title,
        summary: state,
        keywords,
        scope: None,
        non_goals: None,
        relations: IndexRelations::default(),
        digest: String::new(),
        tags,
        generated_at: Utc::now(),
        source_modified_at: Some(s.updated_at),
    }
}

fn extract_keywords(title: &str) -> Vec<String> {
    let mut kw: Vec<String> = title
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .map(|w| w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty())
        .collect();
    kw.sort_unstable();
    kw.dedup();
    kw
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use chrono::Utc;
    use uuid::Uuid;
    use crate::storage::indexed::IndexedEntity;

    fn fake_spec(id: Uuid, title: &str, state: &str, path: PathBuf) -> IndexedEntity {
        IndexedEntity {
            id,
            path,
            type_id: "specification".to_string(),
            title: Some(title.to_string()),
            state: Some(state.to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn generate_spec_sidecar_basic() {
        let ws = PathBuf::from("/workspace");
        let id = Uuid::new_v4();
        let specs = vec![fake_spec(id, "My Spec", "active", ws.join(".spec/entities/id/spec.toml"))];

        let sidecar = generate_spec_sidecar(SpecIndexInput {
            specs: &specs,
            workspace_root: &ws,
            store_dir: ".spec",
        });

        assert_eq!(sidecar.entries.len(), 1);
        assert_eq!(sidecar.entries[0].kind, ContentKind::Spec);
        assert_eq!(sidecar.entries[0].tags, vec!["active".to_string()]);
        assert!(sidecar.entries[0].is_digest_valid());
        assert!(!sidecar.entries[0].source_path.contains('\\'));
    }
}
