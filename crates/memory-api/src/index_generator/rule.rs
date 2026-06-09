//! Rule store catalog generator (ticket `9336a096`).
//!
//! Converts indexed rule entities into an [`IndexSidecar`] for `.rule/index.toon`.
//! Rules are grouped by slug-prefix segments (D4) in the sidecar `tags` field so
//! consumers can filter by group without parsing the slug themselves.

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

/// Input for the rule sidecar generator.
pub struct RuleIndexInput<'a> {
    /// All non-deleted indexed rule entities.
    pub rules: &'a [IndexedEntity],
    /// Workspace root for relative path computation.
    pub workspace_root: &'a Path,
    /// Store folder relative to workspace root (default: `.rule`).
    pub store_dir: &'a str,
}

/// Generate an [`IndexSidecar`] for the rule store.
///
/// Each rule entity becomes one [`IndexEntry`] with:
/// - `kind` = [`ContentKind::Rule`]
/// - `source_path` = workspace-relative path (using `/` separators)
/// - `title` from indexed metadata
/// - `tags` include the rule's slug prefix segments (D4 grouping) and state
///
/// All entries are sealed and sorted by id.
pub fn generate_rule_sidecar(input: RuleIndexInput<'_>) -> IndexSidecar {
    let mut entries: Vec<IndexEntry> = input
        .rules
        .iter()
        .filter(|r| !r.deleted)
        .map(|r| make_rule_entry(r, input.workspace_root))
        .collect();

    for e in &mut entries {
        e.seal();
    }

    let mut sidecar = IndexSidecar::new(
        ContentKind::Rule,
        input.store_dir,
        entries,
    );
    sidecar.sort();
    sidecar
}

fn make_rule_entry(
    r: &IndexedEntity,
    workspace_root: &Path,
) -> IndexEntry {
    let source_path = to_relative_slash(workspace_root, &r.path);
    let title = r.title.clone().unwrap_or_else(|| r.id.to_string());
    let state = r.state.clone().unwrap_or_default();

    // Tags: slug-prefix group segments (D4) + state.
    // The slug can often be derived from the folder name in source_path.
    // We use whatever segments precede the last `/` component as the group prefix.
    let mut tags: Vec<String> = slug_prefix_tags(&source_path);
    if !state.is_empty() {
        tags.push(state.clone());
    }
    tags.sort_unstable();
    tags.dedup();

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
        id: r.id,
        kind: ContentKind::Rule,
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
        source_modified_at: Some(r.updated_at),
    }
}

/// Extract slug-prefix group tags from a `/`-separated relative path.
///
/// For a rule at `.rule/entities/shared/agent-rules/some-rule/rule.toml`,
/// the prefix segments are `["shared", "agent-rules"]`.
fn slug_prefix_tags(source_path: &str) -> Vec<String> {
    // Strip the store prefix (e.g. `.rule/entities/`) and the leaf file name.
    let parts: Vec<&str> = source_path.split('/').collect();
    // Convention: `.rule/entities/<slug-segments...>/<id-or-leaf>/rule.toml`
    // We want everything between `entities/` and the last two components.
    let entities_pos = parts.iter().position(|p| *p == "entities");
    let Some(start) = entities_pos else {
        return vec![];
    };
    // parts after "entities", minus the last two (uuid folder + file name)
    let slug_parts = &parts[start + 1..];
    if slug_parts.len() <= 2 {
        return vec![];
    }
    slug_parts[..slug_parts.len() - 2]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use chrono::Utc;
    use uuid::Uuid;
    use crate::storage::indexed::IndexedEntity;

    fn fake_rule(id: Uuid, title: &str, path: PathBuf) -> IndexedEntity {
        IndexedEntity {
            id,
            path,
            type_id: "rule-entry".to_string(),
            title: Some(title.to_string()),
            state: Some("active".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted: false,
        }
    }

    #[test]
    fn slug_prefix_tags_extracts_segments() {
        let tags = slug_prefix_tags(".rule/entities/shared/agent-rules/some-rule/rule.toml");
        assert_eq!(tags, vec!["shared".to_string(), "agent-rules".to_string()]);
    }

    #[test]
    fn slug_prefix_tags_shallow_path() {
        let tags = slug_prefix_tags(".rule/entities/rule.toml");
        assert!(tags.is_empty());
    }

    #[test]
    fn generate_rule_sidecar_basic() {
        let ws = PathBuf::from("/workspace");
        let id = Uuid::new_v4();
        let rules = vec![fake_rule(
            id,
            "My Rule",
            ws.join(".rule/entities/shared/my-rule/rule.toml"),
        )];

        let sidecar = generate_rule_sidecar(RuleIndexInput {
            rules: &rules,
            workspace_root: &ws,
            store_dir: ".rule",
        });

        assert_eq!(sidecar.entries.len(), 1);
        assert_eq!(sidecar.entries[0].kind, ContentKind::Rule);
        assert!(sidecar.entries[0].tags.contains(&"shared".to_string()));
        assert!(sidecar.entries[0].is_digest_valid());
    }
}
