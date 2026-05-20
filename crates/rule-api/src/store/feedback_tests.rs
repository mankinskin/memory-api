use std::fs;

use tempfile::tempdir;

use crate::{
    FeedbackNoteKind,
    FeedbackRating,
    RuleFeedbackEvent,
    RuleFeedbackInput,
    RuleManifest,
    RuleStore,
};

#[test]
fn record_feedback_appends_event_log_and_updates_summary() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let rule = RuleManifest::new(
        "shared/agents/feedback",
        "Feedback",
        "AGENTS",
        "feedback",
        "Capture ratings.",
    );
    let id = store.create(&rule, None).unwrap();

    let (updated, first_event) = store
        .record_feedback(
            &id.to_string(),
            RuleFeedbackInput::new(
                FeedbackRating::NotHelpful,
                Some("Needs a clearer usage example.".to_string()),
                Some(FeedbackNoteKind::Suggestion),
                Some("session-123".to_string()),
                Some("copilot-gpt-5.4".to_string()),
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(updated.feedback_helpful_count(), Some(0));
    assert_eq!(updated.feedback_mixed_count(), Some(0));
    assert_eq!(updated.feedback_not_helpful_count(), Some(1));
    assert_eq!(updated.feedback_note_count(), Some(1));
    assert_eq!(updated.feedback_unresolved_count(), Some(1));
    assert_eq!(
        updated.feedback_last_at(),
        Some(first_event.timestamp.as_str())
    );

    let log_path = dir
        .path()
        .join("rules")
        .join(id.to_string())
        .join("assets")
        .join("feedback")
        .join("events.ndjson");
    let first_log = read_event_log(&log_path);
    assert_eq!(first_log, vec![first_event.clone()]);

    let (updated, second_event) = store
        .record_feedback(
            rule.slug().unwrap(),
            RuleFeedbackInput::new(
                FeedbackRating::Helpful,
                None,
                None,
                None,
                None,
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(updated.feedback_helpful_count(), Some(1));
    assert_eq!(updated.feedback_mixed_count(), Some(0));
    assert_eq!(updated.feedback_not_helpful_count(), Some(1));
    assert_eq!(updated.feedback_note_count(), Some(1));
    assert_eq!(updated.feedback_unresolved_count(), Some(1));
    assert_eq!(
        updated.feedback_last_at(),
        Some(second_event.timestamp.as_str())
    );

    let second_log = read_event_log(&log_path);
    assert_eq!(second_log, vec![first_event, second_event]);
}

fn read_event_log(path: &std::path::Path) -> Vec<RuleFeedbackEvent> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}