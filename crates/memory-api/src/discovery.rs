//! Recursive multi-store workspace discovery.
//!
//! Walks a workspace tree and finds every store marker directory (`.ticket`,
//! `.spec`, `.rule`, `.test`, `.audit`, …), mapping each to its
//! [`ContentKind`]. Discovery is fully automatic and recursive so a parent
//! workspace transparently sees stores in nested submodule workspaces.
//!
//! Traversal is:
//! - **loop-safe** — symlinks are followed only after canonical-path dedup, so
//!   a directory cycle cannot diverge.
//! - **bounded** — heavy/irrelevant trees (`target`, `node_modules`, `.git`, …)
//!   are pruned, and a depth limit caps deep hierarchies.
//! - **deduplicated** — the same canonical store root is reported once.
//!
//! The discovered roots feed indexing/reconciliation and the [`Urn`](crate::Urn)
//! cross-store reference model: each store maps `workspace + ContentKind` to a
//! filesystem location.

use std::{
    collections::BTreeSet,
    path::{
        Path,
        PathBuf,
    },
};

use crate::model::index_entry::ContentKind;

/// Maximum directory depth descended from the discovery root.
pub const MAX_DISCOVERY_DEPTH: usize = 24;

/// Known store-marker directory names and the [`ContentKind`] they host.
pub const STORE_MARKERS: &[(&str, ContentKind)] = &[
    (".ticket", ContentKind::Ticket),
    (".spec", ContentKind::Spec),
    (".rule", ContentKind::Rule),
    (".test", ContentKind::Test),
    (".audit", ContentKind::AuditFinding),
];

/// A single store found during discovery.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiscoveredStore {
    /// The store-marker directory (e.g. `<workspace>/.ticket`).
    pub store_root: PathBuf,
    /// The workspace root owning the store (the marker's parent).
    pub workspace_root: PathBuf,
    /// Domain hosted by the store.
    pub kind: ContentKind,
}

/// Recursively discover every store under `root`, deduplicated and loop-safe.
///
/// Results are sorted by `(store_root, kind)` for deterministic output.
pub fn discover_stores(root: &Path) -> Vec<DiscoveredStore> {
    let mut found = Vec::new();
    let mut visited = BTreeSet::new();
    walk(root, 0, &mut visited, &mut found);
    found.sort();
    found.dedup();
    found
}

fn walk(
    dir: &Path,
    depth: usize,
    visited: &mut BTreeSet<PathBuf>,
    out: &mut Vec<DiscoveredStore>,
) {
    if depth > MAX_DISCOVERY_DEPTH {
        return;
    }

    // Loop-safety: dedup on canonical path so a symlink cycle terminates.
    let canonical =
        std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(canonical) {
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if let Some(kind) = store_kind_for(name) {
            out.push(DiscoveredStore {
                workspace_root: dir.to_path_buf(),
                store_root: path.clone(),
                kind,
            });
            // Store internals never contain nested workspaces; skip recursion.
            continue;
        }

        if should_skip_dir(name) {
            continue;
        }

        walk(&path, depth + 1, visited, out);
    }
}

fn store_kind_for(name: &str) -> Option<ContentKind> {
    STORE_MARKERS
        .iter()
        .find(|(marker, _)| *marker == name)
        .map(|(_, kind)| *kind)
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".hg" | ".svn" | "target" | "node_modules" | "release" | "tmp"
    )
}

/// Integration state of a store across a reconcile against a prior snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IntegrationStatus {
    /// Present in the prior snapshot and still present now.
    Integrated,
    /// Newly present this pass (absent-then-present onboarding).
    Discovered,
    /// Present before but missing now — surfaced as a diagnostic, not rebuilt.
    Absent,
}

/// Per-store reconciliation outcome with stable integration status.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StoreReport {
    pub store_root: PathBuf,
    pub workspace_root: PathBuf,
    pub kind: ContentKind,
    pub status: IntegrationStatus,
}

/// Aggregate counts for scan/index reporting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileSummary {
    pub discovered: usize,
    pub integrated: usize,
    pub diagnostic: usize,
}

/// Reconcile a prior set of known stores against a fresh discovery under `root`.
///
/// Non-destructive: a store missing now is reported `Absent` (diagnostic) rather
/// than dropped, and a newly appeared store is `Discovered`, so late onboarding
/// needs no rebuild. Reports are sorted for deterministic scan output.
pub fn reconcile_stores(
    previous: &[DiscoveredStore],
    root: &Path,
) -> Vec<StoreReport> {
    let current = discover_stores(root);
    let prev: BTreeSet<_> =
        previous.iter().map(|s| s.store_root.clone()).collect();
    let cur: BTreeSet<_> =
        current.iter().map(|s| s.store_root.clone()).collect();

    let mut reports: Vec<StoreReport> = current
        .iter()
        .map(|s| StoreReport {
            store_root: s.store_root.clone(),
            workspace_root: s.workspace_root.clone(),
            kind: s.kind,
            status: if prev.contains(&s.store_root) {
                IntegrationStatus::Integrated
            } else {
                IntegrationStatus::Discovered
            },
        })
        .collect();

    for s in previous.iter().filter(|s| !cur.contains(&s.store_root)) {
        reports.push(StoreReport {
            store_root: s.store_root.clone(),
            workspace_root: s.workspace_root.clone(),
            kind: s.kind,
            status: IntegrationStatus::Absent,
        });
    }

    reports.sort();
    reports
}

/// Summarize reconcile reports into scan-level counters.
pub fn summarize(reports: &[StoreReport]) -> ReconcileSummary {
    let mut s = ReconcileSummary::default();
    for r in reports {
        match r.status {
            IntegrationStatus::Discovered => s.discovered += 1,
            IntegrationStatus::Integrated => s.integrated += 1,
            IntegrationStatus::Absent => s.diagnostic += 1,
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn mk(
        root: &Path,
        rel: &str,
    ) {
        std::fs::create_dir_all(root.join(rel)).unwrap();
    }

    #[test]
    fn discovers_nested_stores_tagged_by_kind() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        mk(root, ".ticket");
        mk(root, ".spec");
        mk(root, "sub/nested/.rule");
        mk(root, "sub/.test");

        let kinds: HashSet<_> =
            discover_stores(root).into_iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&ContentKind::Ticket));
        assert!(kinds.contains(&ContentKind::Spec));
        assert!(kinds.contains(&ContentKind::Rule));
        assert!(kinds.contains(&ContentKind::Test));
    }

    #[test]
    fn prunes_heavy_dirs_and_does_not_recurse_into_stores() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        mk(root, "target/.spec");
        mk(root, "node_modules/pkg/.ticket");
        mk(root, ".ticket/tickets/abc/.spec");

        let found = discover_stores(root);
        // Only the top-level .ticket; pruned dirs and store internals excluded.
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ContentKind::Ticket);
        assert_eq!(found[0].store_root, root.join(".ticket"));
    }

    #[test]
    fn deduplicates_repeated_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        mk(root, ".spec");
        let a = discover_stores(root);
        let b = discover_stores(root);
        assert_eq!(a, b);
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn empty_tree_yields_no_stores() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(discover_stores(tmp.path()).is_empty());
    }

    #[test]
    fn absent_then_present_onboards_without_rebuild() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        mk(root, ".ticket");

        // First pass: ticket present, spec referenced but absent.
        let first = discover_stores(root);
        assert_eq!(first.len(), 1);

        // A reference to a spec store that does not exist yet.
        let absent_spec = vec![DiscoveredStore {
            store_root: root.join(".spec"),
            workspace_root: root.to_path_buf(),
            kind: ContentKind::Spec,
        }];
        let reports = reconcile_stores(&absent_spec, root);
        let spec = reports
            .iter()
            .find(|r| r.kind == ContentKind::Spec)
            .unwrap();
        assert_eq!(spec.status, IntegrationStatus::Absent);
        assert_eq!(summarize(&reports).diagnostic, 1);

        // Spec store appears later; reconcile integrates it without rebuild.
        mk(root, ".spec");
        let known = discover_stores(root);
        let reports = reconcile_stores(&known, root);
        let spec = reports
            .iter()
            .find(|r| r.kind == ContentKind::Spec)
            .unwrap();
        assert_eq!(spec.status, IntegrationStatus::Integrated);
        assert_eq!(summarize(&reports).diagnostic, 0);
    }

    #[test]
    fn newly_added_store_is_discovered() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        mk(root, ".ticket");
        let prev = discover_stores(root);
        mk(root, ".rule");
        let reports = reconcile_stores(&prev, root);
        let rule = reports
            .iter()
            .find(|r| r.kind == ContentKind::Rule)
            .unwrap();
        assert_eq!(rule.status, IntegrationStatus::Discovered);
        assert_eq!(summarize(&reports).discovered, 1);
        assert_eq!(summarize(&reports).integrated, 1);
    }
}
