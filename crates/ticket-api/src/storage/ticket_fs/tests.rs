use std::collections::BTreeMap;

use serde_json::Value;

use super::HistoryRevision;

#[test]
fn history_revision_backward_compat_no_author() {
    let json = r#"{"rev":1,"ts":"2025-01-01T00:00:00Z","fields":{"state":"new","title":"Old entry"}}"#;
    let rev: HistoryRevision = serde_json::from_str(json)
        .expect("should deserialize legacy revision without author field");
    assert_eq!(rev.rev, 1);
    assert_eq!(rev.author, None, "author should be None for legacy entries");
}

#[test]
fn history_revision_with_author() {
    let json =
        r#"{"rev":2,"ts":"2025-01-02T00:00:00Z","fields":{},"author":"alice"}"#;
    let rev: HistoryRevision = serde_json::from_str(json)
        .expect("should deserialize revision with author");
    assert_eq!(rev.author, Some("alice".to_string()));
}

#[test]
fn history_revision_none_author_is_skipped_in_serialization() {
    let rev = HistoryRevision {
        rev: 1,
        ts: "2025-01-01T00:00:00Z".to_string(),
        fields: BTreeMap::new(),
        author: None,
    };
    let json = serde_json::to_string(&rev).expect("serialize");
    let value: Value = serde_json::from_str(&json).unwrap();
    assert!(
        value.get("author").is_none(),
        "author key should be absent when None"
    );
}
