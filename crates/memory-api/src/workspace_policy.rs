//! Explicit workspace-policy layer for scan-root discovery.
//!
//! The policy is loaded from `<workspace_root>/.ticket/workspace-policy.toml`
//! and governs which workspaces are discovered, scanned, and queried. This
//! module owns parsing and in-memory representation only; wiring into the
//! discovery, scan, and query paths is handled by later slices.
//!
//! When the policy file is absent, the loader returns compatibility-mode
//! defaults that mirror current discovery behavior (descendants and ancestors
//! both included) and emits a single warning recommending an explicit policy.

use std::path::Path;

use globset::{
    Glob,
    GlobSet,
    GlobSetBuilder,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::workspace::TICKET_INDEX_DIR;

/// Policy file name, resolved under `<workspace_root>/.ticket/`.
pub const WORKSPACE_POLICY_FILE: &str = "workspace-policy.toml";

const fn default_true() -> bool {
    true
}

fn default_ignore_markers() -> Vec<String> {
    vec![
        ".ticket-ignore".to_string(),
        ".workspace-ignore".to_string(),
    ]
}

/// In-memory representation of `.ticket/workspace-policy.toml`.
///
/// Per-field defaults are applied for partial files via `#[serde(default)]`
/// on each field, so an empty or partial policy still yields the documented
/// defaults: descendants included, ancestors excluded, external paths denied.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspacePolicy {
    /// Include descendant stores discovered beneath the workspace root.
    #[serde(default = "default_true")]
    pub include_descendants: bool,

    /// Include ancestor stores discovered above the workspace root.
    ///
    /// Defaults to `false` (safer). Compatibility mode overrides this to
    /// `true` to preserve current behavior when no policy file exists.
    #[serde(default)]
    pub include_ancestors: bool,

    /// Hard security boundary: refuse to index roots outside the workspace
    /// subtree when enabled.
    #[serde(default = "default_true")]
    pub deny_external_paths: bool,

    /// Glob or relative-path patterns for workspaces to exclude.
    #[serde(default)]
    pub ignore_workspaces: Vec<String>,

    /// Glob or relative-path patterns that force inclusion, winning over
    /// `ignore_workspaces` and marker files.
    #[serde(default)]
    pub include_overrides: Vec<String>,

    /// Marker file names that, when present in a candidate workspace, opt it
    /// out of discovery unless an include override applies.
    #[serde(default = "default_ignore_markers")]
    pub ignore_markers: Vec<String>,

    /// Whether this policy was synthesized from compatibility-mode defaults
    /// (no policy file present). Not read from or written to the file.
    #[serde(skip)]
    pub compatibility_mode: bool,
}

impl Default for WorkspacePolicy {
    fn default() -> Self {
        Self {
            include_descendants: true,
            include_ancestors: false,
            deny_external_paths: true,
            ignore_workspaces: Vec::new(),
            include_overrides: Vec::new(),
            ignore_markers: default_ignore_markers(),
            compatibility_mode: false,
        }
    }
}

impl WorkspacePolicy {
    /// Compatibility-mode defaults used when no policy file is present.
    ///
    /// Mirrors current discovery behavior by including both descendants and
    /// ancestors (which requires permitting external paths), and marks the
    /// policy as compatibility-derived.
    pub fn compatibility_default() -> Self {
        Self {
            include_ancestors: true,
            deny_external_paths: false,
            compatibility_mode: true,
            ..Self::default()
        }
    }

    /// Returns `true` when `rel_path` matches any `ignore_workspaces` pattern.
    pub fn matches_ignore(
        &self,
        rel_path: &Path,
    ) -> bool {
        matches_any(&self.ignore_workspaces, rel_path)
    }

    /// Returns `true` when `rel_path` matches any `include_overrides` pattern.
    pub fn matches_include_override(
        &self,
        rel_path: &Path,
    ) -> bool {
        matches_any(&self.include_overrides, rel_path)
    }

    /// Returns `true` when `candidate_dir` contains any configured ignore
    /// marker file.
    pub fn has_ignore_marker(
        &self,
        candidate_dir: &Path,
    ) -> bool {
        self.ignore_markers
            .iter()
            .any(|marker| candidate_dir.join(marker).is_file())
    }

    /// Resolve whether a candidate workspace is ignored by policy.
    ///
    /// A candidate is ignored when it matches an `ignore_workspaces` glob or
    /// carries an ignore marker, unless an `include_overrides` pattern applies
    /// (overrides always win).
    pub fn is_ignored(
        &self,
        rel_path: &Path,
        candidate_dir: &Path,
    ) -> bool {
        if self.matches_include_override(rel_path) {
            return false;
        }
        self.matches_ignore(rel_path) || self.has_ignore_marker(candidate_dir)
    }
}

/// Load the workspace policy for `workspace_root`.
///
/// - Present file: parsed and authoritative (`compatibility_mode = false`).
/// - Absent file: compatibility-mode defaults with a single warning.
/// - Malformed file: warns and falls back to compatibility-mode defaults.
pub fn load_workspace_policy(workspace_root: &Path) -> WorkspacePolicy {
    let policy_path = workspace_root
        .join(TICKET_INDEX_DIR)
        .join(WORKSPACE_POLICY_FILE);

    match std::fs::read_to_string(&policy_path) {
        Ok(contents) => match toml::from_str::<WorkspacePolicy>(&contents) {
            Ok(mut policy) => {
                policy.compatibility_mode = false;
                policy
            },
            Err(error) => {
                tracing::warn!(
                    path = %policy_path.display(),
                    %error,
                    "failed to parse workspace-policy.toml; using compatibility-mode defaults"
                );
                WorkspacePolicy::compatibility_default()
            },
        },
        Err(_) => {
            tracing::warn!(
                path = %policy_path.display(),
                "no .ticket/workspace-policy.toml found; using compatibility-mode discovery (descendants + ancestors). Add an explicit policy to control scan-root inclusion."
            );
            WorkspacePolicy::compatibility_default()
        },
    }
}

/// Load the on-disk policy file only, without compatibility fallback.
///
/// Returns `None` when the file is absent, and a parsed policy otherwise
/// (malformed files yield `WorkspacePolicy::default()` so editing callers never
/// silently drop all fields). Intended for mutation flows that should start
/// from documented defaults rather than compatibility-mode defaults.
pub fn load_workspace_policy_file(
    workspace_root: &Path
) -> Option<WorkspacePolicy> {
    let policy_path = workspace_root
        .join(TICKET_INDEX_DIR)
        .join(WORKSPACE_POLICY_FILE);
    let contents = std::fs::read_to_string(&policy_path).ok()?;
    Some(toml::from_str(&contents).unwrap_or_default())
}

/// Persist `policy` to `<workspace_root>/.ticket/workspace-policy.toml`.
///
/// Creates the `.ticket/` directory when absent and serializes deterministically
/// via the [`WorkspacePolicy`] type (field-ordered TOML), so `set`/`add`/`remove`
/// operations round-trip through the typed model rather than raw text editing.
pub fn save_workspace_policy(
    workspace_root: &Path,
    policy: &WorkspacePolicy,
) -> std::io::Result<()> {
    let ticket_dir = workspace_root.join(TICKET_INDEX_DIR);
    std::fs::create_dir_all(&ticket_dir)?;
    let policy_path = ticket_dir.join(WORKSPACE_POLICY_FILE);
    let contents = toml::to_string_pretty(policy).map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, error)
    })?;
    std::fs::write(policy_path, contents)
}
///
/// Invalid patterns are skipped so a single bad entry does not disable the
/// entire set.
fn build_globset(patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let normalized = pattern.replace('\\', "/");
        if let Ok(glob) = Glob::new(&normalized) {
            builder.add(glob);
        }
    }
    builder.build().unwrap_or_else(|_| GlobSet::empty())
}

/// Normalize a relative path to a forward-slash string for glob matching.
fn normalize_rel(rel_path: &Path) -> String {
    rel_path.to_string_lossy().replace('\\', "/")
}

fn matches_any(
    patterns: &[String],
    rel_path: &Path,
) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let candidate = normalize_rel(rel_path);
    build_globset(patterns).is_match(candidate)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn parses_full_policy_file() {
        let toml = r#"
include_descendants = false
include_ancestors = true
deny_external_paths = false
ignore_workspaces = ["fixtures/**", "test-*"]
include_overrides = ["fixtures/keep"]
ignore_markers = [".skip"]
"#;
        let policy: WorkspacePolicy = toml::from_str(toml).unwrap();
        assert!(!policy.include_descendants);
        assert!(policy.include_ancestors);
        assert!(!policy.deny_external_paths);
        assert_eq!(
            policy.ignore_workspaces,
            vec!["fixtures/**".to_string(), "test-*".to_string()]
        );
        assert_eq!(policy.include_overrides, vec!["fixtures/keep".to_string()]);
        assert_eq!(policy.ignore_markers, vec![".skip".to_string()]);
        assert!(!policy.compatibility_mode);
    }

    #[test]
    fn applies_documented_defaults_for_partial_file() {
        let policy: WorkspacePolicy =
            toml::from_str("ignore_workspaces = [\"foo\"]").unwrap();
        assert!(policy.include_descendants);
        assert!(!policy.include_ancestors);
        assert!(policy.deny_external_paths);
        assert_eq!(policy.include_overrides, Vec::<String>::new());
        assert_eq!(policy.ignore_markers, default_ignore_markers());
    }

    #[test]
    fn empty_file_yields_all_defaults() {
        let policy: WorkspacePolicy = toml::from_str("").unwrap();
        assert_eq!(policy, WorkspacePolicy::default());
    }

    #[test]
    fn absent_file_yields_compatibility_mode() {
        let dir = tempdir().unwrap();
        let policy = load_workspace_policy(dir.path());
        assert!(policy.compatibility_mode);
        assert!(policy.include_descendants);
        assert!(policy.include_ancestors);
        // Compatibility mode permits external (ancestor) paths for parity.
        assert!(!policy.deny_external_paths);
    }

    #[test]
    fn present_file_is_authoritative() {
        let dir = tempdir().unwrap();
        let ticket_dir = dir.path().join(TICKET_INDEX_DIR);
        std::fs::create_dir_all(&ticket_dir).unwrap();
        std::fs::write(
            ticket_dir.join(WORKSPACE_POLICY_FILE),
            "include_ancestors = false\nignore_workspaces = [\"vendor/**\"]\n",
        )
        .unwrap();

        let policy = load_workspace_policy(dir.path());
        assert!(!policy.compatibility_mode);
        assert!(!policy.include_ancestors);
        assert!(policy.matches_ignore(&PathBuf::from("vendor/dep")));
    }

    #[test]
    fn malformed_file_falls_back_to_compatibility_mode() {
        let dir = tempdir().unwrap();
        let ticket_dir = dir.path().join(TICKET_INDEX_DIR);
        std::fs::create_dir_all(&ticket_dir).unwrap();
        std::fs::write(
            ticket_dir.join(WORKSPACE_POLICY_FILE),
            "include_descendants = \"not-a-bool\"\n",
        )
        .unwrap();

        let policy = load_workspace_policy(dir.path());
        assert!(policy.compatibility_mode);
        assert_eq!(policy, WorkspacePolicy::compatibility_default());
    }

    #[test]
    fn glob_matches_and_non_matches() {
        let policy = WorkspacePolicy {
            ignore_workspaces: vec![
                "fixtures/**".to_string(),
                "test-*".to_string(),
            ],
            ..WorkspacePolicy::default()
        };
        assert!(policy.matches_ignore(&PathBuf::from("fixtures/a/b")));
        assert!(policy.matches_ignore(&PathBuf::from("test-store")));
        assert!(!policy.matches_ignore(&PathBuf::from("crates/core")));
    }

    #[test]
    fn include_override_wins_over_ignore() {
        let dir = tempdir().unwrap();
        let policy = WorkspacePolicy {
            ignore_workspaces: vec!["fixtures/**".to_string()],
            include_overrides: vec!["fixtures/keep".to_string()],
            ..WorkspacePolicy::default()
        };
        let ignored = PathBuf::from("fixtures/drop");
        let kept = PathBuf::from("fixtures/keep");
        assert!(policy.is_ignored(&ignored, dir.path()));
        assert!(!policy.is_ignored(&kept, dir.path()));
    }

    #[test]
    fn ignore_marker_detected_and_overridable() {
        let dir = tempdir().unwrap();
        let candidate = dir.path().join("child");
        std::fs::create_dir_all(&candidate).unwrap();
        std::fs::write(candidate.join(".ticket-ignore"), "").unwrap();

        let policy = WorkspacePolicy::default();
        assert!(policy.has_ignore_marker(&candidate));
        assert!(policy.is_ignored(&PathBuf::from("child"), &candidate));

        let with_override = WorkspacePolicy {
            include_overrides: vec!["child".to_string()],
            ..WorkspacePolicy::default()
        };
        assert!(!with_override.is_ignored(&PathBuf::from("child"), &candidate));
    }

    #[test]
    fn backslash_patterns_normalized() {
        let policy = WorkspacePolicy {
            ignore_workspaces: vec!["fixtures\\**".to_string()],
            ..WorkspacePolicy::default()
        };
        assert!(policy.matches_ignore(&PathBuf::from("fixtures/nested")));
    }
}
