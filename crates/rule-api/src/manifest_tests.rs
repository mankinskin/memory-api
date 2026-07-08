use super::*;

#[test]
fn new_manifest_sets_uuid_and_slug_fields() {
    let manifest = RuleManifest::new(
        "shared/agents/discovery-protocol",
        "Discovery Protocol",
        "agents",
        "discovery-protocol",
        "Use live sources first.",
    );

    assert_eq!(manifest.slug(), Some("shared/agents/discovery-protocol"));
    assert_eq!(manifest.title(), Some("Discovery Protocol"));
    assert_eq!(manifest.file_kind(), Some("agents"));
    assert_eq!(manifest.section(), Some("discovery-protocol"));
    assert_eq!(manifest.state(), Some("draft"));
    assert!(manifest.id != Uuid::nil());
}

#[test]
fn manifest_supports_rule_metadata_fields() {
    let mut manifest = RuleManifest::new(
        "shared/github/readme/overview",
        "Overview",
        ".github/README",
        "overview",
        "Canonical project overview.",
    );
    manifest.set_order_key(20);
    manifest.set_repo_scopes(["context-engine", "memory-api"]);
    manifest.set_path_scopes([".github/**", "AGENTS.md"]);
    manifest.set_sentence_anchors(["p1-s1", "p1-s2"]);
    manifest.set_source_location("context-engine", ".github/README.md", 4, 12);
    manifest.set_feedback_summary(3, 1, 0, 2, 1, Some("2026-05-07T14:00:00Z"));

    assert_eq!(manifest.order_key(), Some(20));
    assert_eq!(
        manifest.repo_scopes(),
        vec!["context-engine", "memory-api"]
    );
    assert_eq!(manifest.path_scopes(), vec![".github/**", "AGENTS.md"]);
    assert_eq!(manifest.sentence_anchors(), vec!["p1-s1", "p1-s2"]);
    assert_eq!(manifest.source_repo(), Some("context-engine"));
    assert_eq!(manifest.source_path(), Some(".github/README.md"));
    assert_eq!(manifest.source_start_line(), Some(4));
    assert_eq!(manifest.source_end_line(), Some(12));
    assert_eq!(manifest.feedback_helpful_count(), Some(3));
    assert_eq!(manifest.feedback_unresolved_count(), Some(1));
    assert_eq!(manifest.feedback_last_at(), Some("2026-05-07T14:00:00Z"));
}
