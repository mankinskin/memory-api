use std::{
    collections::BTreeMap,
    path::Path,
};

use feedback_api::{
    EntityFeedbackStore,
    EntityUrn,
    FeedbackEntry,
    FeedbackNoteKind,
    FeedbackProvenance,
    FeedbackRating,
    FeedbackSource,
};
use serde_json::Value;
use uuid::Uuid;

use crate::storage::TicketStore;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SituationQuery {
    pub query: String,
    pub context_tags: Vec<String>,
}

/// Open a missing-rule ticket and persist linked feedback when no rule matched.
pub fn handle_missing_rule_match(
    ticket_store: &TicketStore,
    feedback_store: &EntityFeedbackStore,
    query_text: &str,
    context_tags: &[String],
    has_matching_rule: bool,
    target_root: Option<&Path>,
) -> Result<Option<Uuid>, String> {
    if has_matching_rule {
        return Ok(None);
    }

    let ticket_id = Uuid::new_v4();
    let title = format!(
        "[missing-rule] Add missing rule for situation: {}",
        query_text
    );
    let description = format!(
        "A session situation query returned no matching rule.\n\n### Query:\n- `{}`\n\n### Context tags:\n- {:?}",
        query_text, context_tags
    );
    let mut extra = BTreeMap::new();
    extra.insert("priority".to_string(), Value::String("medium".to_string()));

    ticket_store
        .create(
            Some(ticket_id),
            "tracker-improvement",
            Some(&title),
            Some("open"),
            extra,
            target_root,
            Some(&description),
        )
        .map_err(|e| format!("Failed to create missing-rule ticket: {e}"))?;

    let ticket_urn = EntityUrn::ticket(
        feedback_store.workspace_slug(),
        ticket_id.to_string(),
    )?;
    let note = format!(
        "no matching rule found for query '{}' with tags {:?}; opened missing-rule ticket {}",
        query_text, context_tags, ticket_id
    );
    let entry = FeedbackEntry::new(
        FeedbackSource::System,
        ticket_urn,
        Some(FeedbackRating::Mixed),
        Some(note),
        Some(FeedbackNoteKind::Suggestion),
        FeedbackProvenance::new(
            None,
            Some("ticket-api/system".to_string()),
            None,
        )?,
    )?;
    let _ = feedback_store.record_entry(entry)?;

    Ok(Some(ticket_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_missing_rule_ticket_and_feedback_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ticket_store =
            TicketStore::open_or_init(dir.path()).expect("ticket store");
        let feedback_store = EntityFeedbackStore::new(dir.path(), "default")
            .expect("feedback store");

        let tags = vec!["session".to_string(), "policy".to_string()];
        let ticket_id = handle_missing_rule_match(
            &ticket_store,
            &feedback_store,
            "no rule for this case",
            &tags,
            false,
            None,
        )
        .expect("handle")
        .expect("ticket id");

        let manifest = ticket_store.get(&ticket_id).expect("manifest");
        let title = manifest
            .extra
            .get("title")
            .and_then(serde_json::Value::as_str)
            .expect("title");
        assert!(title.contains("missing-rule"));

        let urn =
            EntityUrn::ticket("default", ticket_id.to_string()).expect("urn");
        let entries = feedback_store.entries_for(&urn).expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, FeedbackSource::System);
    }

    #[test]
    fn returns_none_when_rule_match_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ticket_store =
            TicketStore::open_or_init(dir.path()).expect("ticket store");
        let feedback_store = EntityFeedbackStore::new(dir.path(), "default")
            .expect("feedback store");
        let tags = vec!["session".to_string()];

        let result = handle_missing_rule_match(
            &ticket_store,
            &feedback_store,
            "already covered",
            &tags,
            true,
            None,
        )
        .expect("handle");

        assert!(result.is_none());
    }
}
