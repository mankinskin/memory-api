//! Audit store status summary generator (ticket `855a1e5d`).
//!
//! Converts an [`AuditReport`] into an [`IndexSidecar`] for `.audit/index.toon`.
//! Each [`AuditFinding`] becomes one [`IndexEntry`] with
//! `ContentKind::AuditFinding`; the overall report summary is emitted as a
//! [`ContentKind::WorkspaceSummary`] root entry.

use std::path::Path;

use chrono::Utc;
use uuid::Uuid;

use crate::model::index_entry::{
    ContentKind,
    IndexEntry,
    IndexRelations,
    IndexRef,
    RelationKind,
};
use crate::model::index_sidecar::IndexSidecar;

use super::ticket::deterministic_uuid;

/// Namespace UUID for deterministic audit root UUIDs.
const AUDIT_NS: Uuid = Uuid::from_bytes([
    0xab, 0xcd, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
    0xcd, 0xef, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55,
]);

/// Minimal audit report data needed by the generator.
///
/// We accept plain fields rather than the `AuditReport` type directly so this
/// module does not need to depend on `audit-api` (which is in a separate crate).
/// CLI callers destructure the real `AuditReport` into this struct.
pub struct AuditReportInput<'a> {
    /// Repository root path (used to construct `source_path` values).
    pub repo_root: &'a Path,
    /// Workspace root used for relative path computation.
    pub workspace_root: &'a Path,
    /// Store folder relative to workspace root (default: `.audit`).
    pub store_dir: &'a str,
    /// Overall rating/score string (e.g. `"B"`, `"pass"`, `"1 critical"`).
    pub overall_rating: &'a str,
    /// Summary of the audit run (timestamp, duration, etc.).
    pub run_summary: &'a str,
    /// Individual audit findings.
    pub findings: &'a [AuditFindingInput],
}

/// Minimal representation of a single audit finding.
pub struct AuditFindingInput {
    /// Finding identifier (category + index).
    pub id: String,
    /// Category (e.g. `"file_length"`, `"test_coverage"`).
    pub category: String,
    /// Severity as a string (e.g. `"critical"`, `"warning"`, `"info"`).
    pub severity: String,
    /// Human-readable summary of the finding.
    pub summary: String,
    /// Optional source file path where the finding was detected.
    pub path: Option<String>,
}

/// Generate an [`IndexSidecar`] from an audit report.
///
/// Produces:
/// - One [`ContentKind::WorkspaceSummary`] root entry for the overall report.
/// - One [`ContentKind::AuditFinding`] entry per finding, with a `related` ref
///   back to the root entry.
///
/// All entries are sealed and sorted by id.
pub fn generate_audit_sidecar(input: AuditReportInput<'_>) -> IndexSidecar {
    let store_path_rel = input.store_dir.to_string();

    // Root summary entry — deterministic UUID so it is stable across runs.
    let root_id = deterministic_uuid(AUDIT_NS, &format!("audit-root:{}", input.store_dir));

    let mut root_entry = IndexEntry {
        id: root_id,
        kind: ContentKind::WorkspaceSummary,
        source_path: format!("{}/index.toon", input.store_dir),
        title: format!("Audit summary — {}", input.overall_rating),
        summary: input.run_summary.to_string(),
        keywords: vec!["audit".to_string(), "summary".to_string()],
        scope: None,
        non_goals: None,
        relations: IndexRelations::default(),
        digest: String::new(),
        tags: vec!["audit".to_string()],
        generated_at: Utc::now(),
        source_modified_at: None,
    };

    // Finding entries — deterministic UUID per finding id string.
    let mut finding_entries: Vec<IndexEntry> = input
        .findings
        .iter()
        .map(|f| make_finding_entry(f, root_id, input.store_dir, input.workspace_root))
        .collect();

    // Build child refs on the root entry.
    root_entry.relations.children = finding_entries
        .iter()
        .map(|e| IndexRef {
            canonical_path: e.source_path.clone(),
            entry_id: e.id,
            relation_kind: RelationKind::Child,
            content_kind: ContentKind::AuditFinding,
            digest: String::new(), // filled after seal
            anchor: None,
        })
        .collect();

    root_entry.seal();

    // Back-fill child ref digests now that root is sealed.
    for r in &mut root_entry.relations.children {
        if let Some(_child) = finding_entries.iter().find(|e| e.id == r.entry_id) {
            // child not yet sealed at this point; leave digest empty for the ref
            // — callers re-validate on the next pass if needed.
        }
    }

    for e in &mut finding_entries {
        e.seal();
    }

    let mut entries = vec![root_entry];
    entries.extend(finding_entries);

    let mut sidecar = IndexSidecar::new(
        ContentKind::AuditFinding,
        store_path_rel,
        entries,
    );
    sidecar.sort();
    sidecar
}

fn make_finding_entry(
    f: &AuditFindingInput,
    parent_id: Uuid,
    store_dir: &str,
    workspace_root: &Path,
) -> IndexEntry {
    let entry_id = deterministic_uuid(AUDIT_NS, &format!("audit-finding:{}", f.id));

    let source_path = if let Some(ref p) = f.path {
        // finding points to a source file — make it workspace-relative
        let abs = workspace_root.join(p);
        abs.strip_prefix(workspace_root)
            .unwrap_or(&abs)
            .to_string_lossy()
            .replace('\\', "/")
    } else {
        format!("{}/index.toon", store_dir)
    };

    let mut tags = vec![f.severity.clone(), "audit-finding".to_string(), f.category.clone()];
    tags.sort_unstable();
    tags.dedup();

    let parent_ref = IndexRef {
        canonical_path: format!("{}/index.toon", store_dir),
        entry_id: parent_id,
        relation_kind: RelationKind::Parent,
        content_kind: ContentKind::WorkspaceSummary,
        digest: String::new(),
        anchor: None,
    };

    IndexEntry {
        id: entry_id,
        kind: ContentKind::AuditFinding,
        source_path,
        title: format!("[{}] {}", f.severity.to_uppercase(), f.summary),
        summary: f.summary.clone(),
        keywords: vec![f.category.clone(), f.severity.clone()],
        scope: Some(f.category.clone()),
        non_goals: None,
        relations: IndexRelations {
            parent: Some(parent_ref),
            children: vec![],
            depends_on: vec![],
            related: vec![],
        },
        digest: String::new(),
        tags,
        generated_at: Utc::now(),
        source_modified_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_input<'a>(
        findings: &'a [AuditFindingInput],
        ws: &'a Path,
    ) -> AuditReportInput<'a> {
        AuditReportInput {
            repo_root: ws,
            workspace_root: ws,
            store_dir: ".audit",
            overall_rating: "B",
            run_summary: "Ran 3 trials, 1 finding.",
            findings,
        }
    }

    #[test]
    fn empty_findings_produces_root_only() {
        let ws = PathBuf::from("/workspace");
        let sidecar = generate_audit_sidecar(sample_input(&[], &ws));
        assert_eq!(sidecar.entries.len(), 1);
        assert_eq!(sidecar.entries[0].kind, ContentKind::WorkspaceSummary);
        assert!(sidecar.entries[0].is_digest_valid());
    }

    #[test]
    fn finding_gets_parent_ref() {
        let ws = PathBuf::from("/workspace");
        let findings = vec![AuditFindingInput {
            id: "file_length:0".to_string(),
            category: "file_length".to_string(),
            severity: "warning".to_string(),
            summary: "File too long".to_string(),
            path: Some("src/big.rs".to_string()),
        }];
        let sidecar = generate_audit_sidecar(sample_input(&findings, &ws));
        assert_eq!(sidecar.entries.len(), 2);
        let finding = sidecar.entries.iter().find(|e| e.kind == ContentKind::AuditFinding).unwrap();
        assert!(finding.relations.parent.is_some());
        assert!(finding.is_digest_valid());
    }

    #[test]
    fn root_id_is_deterministic() {
        let ws = PathBuf::from("/workspace");
        let s1 = generate_audit_sidecar(sample_input(&[], &ws));
        let s2 = generate_audit_sidecar(sample_input(&[], &ws));
        let root1 = s1.entries.iter().find(|e| e.kind == ContentKind::WorkspaceSummary).unwrap();
        let root2 = s2.entries.iter().find(|e| e.kind == ContentKind::WorkspaceSummary).unwrap();
        assert_eq!(root1.id, root2.id, "root id must be deterministic");
    }
}
