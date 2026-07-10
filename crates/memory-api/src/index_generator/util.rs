//! Generic, domain-agnostic helpers shared by every store-index generator.
//!
//! These helpers were promoted out of `index_generator/ticket.rs` so that
//! domain-owned generators (which live in their own crates, e.g. `rule-api`)
//! can reuse them without depending on the ticket generator (decision Q1.1 of
//! the `thin-generator-architecture` spec).

use std::path::Path;

use uuid::Uuid;

/// Convert an absolute path to a workspace-relative string with `/` separators.
///
/// When `abs_path` is not under `workspace_root` the original path is returned
/// (with separators normalized). Output is always `/`-separated so generated
/// artifacts are byte-identical across platforms.
pub fn to_relative_slash(
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
/// Deterministic: identical inputs always yield the same UUID (UUID v5).
pub fn deterministic_uuid(
    namespace: Uuid,
    slug: &str,
) -> Uuid {
    Uuid::new_v5(&namespace, slug.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn to_relative_slash_strips_root_and_normalizes() {
        let root = PathBuf::from("/workspace");
        let abs = PathBuf::from("/workspace/.rule/entries/x/rule.toml");
        assert_eq!(to_relative_slash(&root, &abs), ".rule/entries/x/rule.toml");
    }

    #[test]
    fn deterministic_uuid_is_stable() {
        let ns = Uuid::nil();
        assert_eq!(
            deterministic_uuid(ns, "shared/agent-rules"),
            deterministic_uuid(ns, "shared/agent-rules")
        );
        assert_ne!(deterministic_uuid(ns, "a"), deterministic_uuid(ns, "b"));
    }
}
