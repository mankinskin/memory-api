use crate::feedback::{
    FeedbackRating,
    FeedbackNoteKind,
    EntityUrn,
};
use std::collections::{HashMap, BTreeMap};
use std::path::Path;
use serde_json::Value;
use uuid::Uuid;

use spec_api::SpecStore;
use test_api::{TestStoreConfig, ValidationOutcome};
use session_api::{SessionTurn, SessionRole};
use ticket_api::storage::TicketStore;

/// Helper function to parse validation guards from spec body markdown.
///
/// Under a `## Guards` heading, lists are parsed to extract backtick-wrapped names.
pub fn parse_guards_from_markdown(body: &str) -> Vec<String> {
    let mut guards = Vec::new();
    let mut in_guards = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#") {
            let heading = trimmed.trim_start_matches('#').trim().to_ascii_lowercase();
            if heading == "guards" {
                in_guards = true;
            } else {
                in_guards = false;
            }
            continue;
        }
        if in_guards {
            if trimmed.starts_with("-") || trimmed.starts_with("*") {
                if let Some(start) = trimmed.find('`') {
                    if let Some(end) = trimmed[start + 1..].find('`') {
                        let guard_val = &trimmed[start + 1..start + 1 + end];
                        guards.push(guard_val.trim().to_string());
                    }
                }
            }
        }
    }
    guards
}

/// Fully wired execution-to-spec verified recommputer.
/// Loads the specification from `SpecStore`, extracts its validation guards,
/// finds latest execution records from `TestStoreConfig` for those guards,
/// and if all latest metrics are passed, transitions the spec state to `"verified"`.
pub fn recompute_spec_verified_state(
    spec_store: &mut SpecStore,
    test_store: &TestStoreConfig,
    spec_id_or_slug: &str,
) -> Result<bool, String> {
    // 1. Get spec manifest and body
    let (_spec, body) = spec_store
        .get_full(spec_id_or_slug)
        .map_err(|e| format!("Spec not found: {e}"))?;

    // 2. Parse guards from body markdown
    let guards = parse_guards_from_markdown(&body);
    if guards.is_empty() {
        return Ok(false);
    }

    // 3. For each guard, retrieve the latest execution from the test store
    let mut latest_executions = HashMap::new();
    let query = test_api::ExecutionQuery {
        limit: None,
        sort: test_api::ExecutionSort::NewestFirst,
        ..Default::default()
    };
    let all_executions = test_store
        .list_executions(&query)
        .map_err(|e| format!("Failed to list executions: {e}"))?;

    for exec in all_executions {
        if guards.contains(&exec.validation_spec_id) {
            let entry = latest_executions.entry(exec.validation_spec_id.clone());
            entry.or_insert(exec);
        }
    }

    // 4. Verify all guards have executed with 'passed'
    if latest_executions.len() < guards.len() {
        return Ok(false);
    }

    let all_passed = guards.iter().all(|guard| {
        if let Some(exec) = latest_executions.get(guard) {
            matches!(exec.outcome, ValidationOutcome::Passed)
        } else {
            false
        }
    });

    if all_passed {
        // Transition spec to "verified"
        spec_store
            .update(spec_id_or_slug, BTreeMap::new(), Some("verified"))
            .map_err(|e| format!("Failed to update spec state to verified: {e}"))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

// ── Transcript Mining Semantics ───────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MiningFeedbackResult {
    pub detected_rule_confusion: bool,
    pub suggested_feedback: Option<String>,
    pub target_entity_urn: Option<EntityUrn>,
}

/// G-D semi-automated ring edge: Transcript Mining -> Feedback Ingestion
///
/// Scans transaction/session transcripts for error patterns or rule names to auto-suggest ratings.
pub fn mine_transcript_for_rule_confusion(
    turns: &[SessionTurn],
    rules: &[(String, String)], // (rule_id_or_slug, rule_body)
) -> Vec<MiningFeedbackResult> {
    let mut results = Vec::new();
    for turn in turns {
        if turn.role == SessionRole::Assistant || turn.role == SessionRole::User {
            let content_lower = turn.content.to_lowercase();
            
            // Smarter heuristics rather than simple contains("error"):
            // Check for actual failures or rule violations being described
            let has_confusion = content_lower.contains("confused in session")
                || content_lower.contains("rule violation")
                || content_lower.contains("incorrectly applied rule")
                || content_lower.contains("policy conflict")
                || (content_lower.contains("error") && content_lower.contains("instruct"));

            if !has_confusion {
                continue;
            }

            for (rule_id, rule_body) in rules {
                // Find distinct, informative context keywords of the rule
                let rule_keywords: Vec<&str> = rule_body
                    .split(|c: char| !c.is_alphabetic())
                    .filter(|w| w.len() > 6)
                    .take(8)
                    .collect();

                let relates_to_rule = rule_keywords.iter().any(|&kw| {
                    let kw_lower = kw.to_lowercase();
                    content_lower.contains(&kw_lower)
                        || (kw_lower.len() >= 5 && content_lower.contains(&kw_lower[..5]))
                });

                if relates_to_rule {
                    if let Ok(urn) = EntityUrn::rule("memory-api", rule_id) {
                        results.push(MiningFeedbackResult {
                            detected_rule_confusion: true,
                            suggested_feedback: Some(format!(
                                "Analysis: rule-confusion detected on {} at sequence {}. Context: {}",
                                rule_id, turn.sequence, turn.content
                            )),
                            target_entity_urn: Some(urn),
                        });
                    }
                }
            }
        }
    }
    results
}

// ── Missing-Rule Auto-Ticketing Semantics ─────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SituationQuery {
    pub query: String,
    pub context_tags: Vec<String>,
}

/// G-D semi-automated ring edge: No-Match -> Missing-Rule Ticket
///
/// If a situation query results in zero matching rules, auto-generate a missing-rule ticket in of the actual ticket store.
pub fn handle_missing_rule_match(
    ticket_store: &TicketStore,
    query_text: &str,
    context_tags: &[String],
    has_matching_rule: bool,
    target_root: Option<&Path>,
) -> Result<Option<Uuid>, String> {
    if !has_matching_rule {
        let ticket_id = Uuid::new_v4();
        let title = format!("[missing-rule] Add missing rule for situation: {}", query_text);
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
                Some("new"),
                extra,
                target_root,
                Some(&description),
            )
            .map_err(|e| format!("Failed to create missing-rule ticket: {e}"))?;
            
        Ok(Some(ticket_id))
    } else {
        Ok(None)
    }
}

// ── User + Web-Frontend Feedback Semantics ────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FrontendFeedbackSubmission {
    pub source_frontend: String, // e.g., "doc-viewer", "log-viewer"
    pub user_id: String,
    pub rating: FeedbackRating,
    pub comments: Option<String>,
    pub target_entity_urn: EntityUrn,
}

/// G-D semi-automated ring edge: Frontend user/agent feedback integration.
pub fn process_frontend_feedback(
    submission: FrontendFeedbackSubmission,
) -> Result<crate::feedback::EntityRatingSubmission, String> {
    Ok(crate::feedback::EntityRatingSubmission {
        rating: submission.rating,
        note_text: submission.comments,
        note_kind: Some(FeedbackNoteKind::Note),
        session_id: Some(format!("frontend-{}", submission.source_frontend)),
        agent_or_user_id: Some(submission.user_id),
    })
}

// ── Direct Ticket Entity Feedback and Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::{EntityFeedbackStore, EntityRatingInput};

    #[test]
    fn test_parse_guards_from_markdown() {
        let md = r#"
<!-- aligned-structure:v2 -->
# Specification

## Guards
The verification of this specification contract is gated by:
- `val-test-auth-mcp` (verifies access)
- `val-visual-render`
"#;
        let parsed = parse_guards_from_markdown(md);
        assert_eq!(parsed, vec!["val-test-auth-mcp", "val-visual-render"]);
    }

    #[test]
    fn test_transcript_mining_logic() {
        let turns = vec![
            SessionTurn {
                sequence: 1,
                role: SessionRole::Assistant,
                content: "I'm confused in session trying to instruct you. It triggered an incorrectly applied rule.".into(),
                captured_at: chrono::Utc::now(),
                tool_name: None,
                model: None,
                event_meta: None,
            }
        ];
        // We match words of length > 6: "instructions" -> matches "instruct"
        let rules = vec![("rule-config".to_string(), "instructions to follow".to_string())];
        let mined = mine_transcript_for_rule_confusion(&turns, &rules);
        assert_eq!(mined.len(), 1);
        assert_eq!(mined[0].target_entity_urn.as_ref().unwrap().entity(), "rule-config");
    }

    #[test]
    fn test_ticket_feedback_and_ratings_in_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = EntityFeedbackStore::new(dir.path(), "test-workspace").unwrap();
        let ticket_urn = EntityUrn::ticket("test-workspace", "ticket-456").unwrap();

        // Count usage
        store.record_usage(ticket_urn.clone()).unwrap();
        let summary_after_usage = store.summary_for(&ticket_urn).unwrap();
        assert_eq!(summary_after_usage.usage_count, 1);

        // Record a rating
        let rating_input = EntityRatingInput::new(
            FeedbackRating::Helpful,
            Some("Ticket resolved perfectly".into()),
            None,
            None,
            None,
        ).unwrap();
        store.record_rating(ticket_urn.clone(), rating_input).unwrap();

        let final_summary = store.summary_for(&ticket_urn).unwrap();
        assert_eq!(final_summary.helpful_count, 1);
        assert_eq!(final_summary.usage_count, 1);
    }
}
