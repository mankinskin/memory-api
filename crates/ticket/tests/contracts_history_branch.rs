use chrono::Utc;
use ticket_api::model::event::{
    BranchLifecycle,
    GitHistoryConfig,
    GitHistoryMode,
    HistoryEntry,
};
use uuid::Uuid;

#[test]
fn git_history_mode_defaults_to_embedded_bare() {
    let cfg = GitHistoryConfig::default();
    assert_eq!(cfg.mode, GitHistoryMode::EmbeddedBare);
}

#[test]
fn history_entry_contains_branch_lifecycle_fields() {
    let entry = HistoryEntry {
        ticket_id: Uuid::new_v4(),
        commit_sha: "deadbeef".to_string(),
        actor: "agent-a".to_string(),
        at: Utc::now(),
        lifecycle: BranchLifecycle {
            created_on_branch: Some("feature/uuid-123".to_string()),
            closed_on_branch: Some("main".to_string()),
            merge_commit: Some("abc123".to_string()),
        },
    };

    let json = serde_json::to_value(&entry).expect("history entry serializes");
    assert_eq!(json["lifecycle"]["created_on_branch"], "feature/uuid-123");
    assert_eq!(json["lifecycle"]["closed_on_branch"], "main");
    assert_eq!(json["lifecycle"]["merge_commit"], "abc123");
}
