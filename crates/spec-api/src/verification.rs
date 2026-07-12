use std::collections::{
    BTreeMap,
    HashMap,
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
use test_api::{
    ExecutionQuery,
    ExecutionSort,
    TestStoreConfig,
    ValidationOutcome,
};

use crate::SpecStore;

/// Parse validation guard ids from markdown under a `## Guards` heading.
pub fn parse_guards_from_markdown(body: &str) -> Vec<String> {
    let mut guards = Vec::new();
    let mut in_guards = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let heading = trimmed.trim_start_matches('#').trim().to_ascii_lowercase();
            in_guards = heading == "guards";
            continue;
        }

        if in_guards && (trimmed.starts_with('-') || trimmed.starts_with('*')) {
            if let Some(start) = trimmed.find('`') {
                if let Some(end) = trimmed[start + 1..].find('`') {
                    guards.push(trimmed[start + 1..start + 1 + end].trim().to_string());
                }
            }
        }
    }
    guards
}

/// Recompute spec `verified` state from guard execution outcomes.
pub fn recompute_spec_verified_state(
    spec_store: &mut SpecStore,
    test_store: &TestStoreConfig,
    feedback_store: Option<&EntityFeedbackStore>,
    spec_id_or_slug: &str,
) -> Result<bool, String> {
    let (_spec, body) = spec_store
        .get_full(spec_id_or_slug)
        .map_err(|e| format!("Spec not found: {e}"))?;

    let guards = parse_guards_from_markdown(&body);
    if guards.is_empty() {
        return Ok(false);
    }

    let query = ExecutionQuery {
        limit: None,
        sort: ExecutionSort::NewestFirst,
        ..Default::default()
    };
    let executions = test_store
        .list_executions(&query)
        .map_err(|e| format!("Failed to list executions: {e}"))?;

    let mut latest_executions = HashMap::new();
    for exec in executions {
        if guards.contains(&exec.validation_spec_id) {
            latest_executions
                .entry(exec.validation_spec_id.clone())
                .or_insert(exec);
        }
    }

    if latest_executions.len() < guards.len() {
        return Ok(false);
    }

    let all_passed = guards.iter().all(|guard| {
        latest_executions
            .get(guard)
            .is_some_and(|exec| matches!(exec.outcome, ValidationOutcome::Passed))
    });

    if !all_passed {
        return Ok(false);
    }

    spec_store
        .update(spec_id_or_slug, BTreeMap::new(), Some("verified"))
        .map_err(|e| format!("Failed to update spec state to verified: {e}"))?;

    if let Some(store) = feedback_store {
        let urn = EntityUrn::spec(store.workspace_slug(), spec_id_or_slug)?;
        let entry = FeedbackEntry::new(
            FeedbackSource::System,
            urn,
            Some(FeedbackRating::Helpful),
            Some("spec guards passed and verified state recomputed".to_string()),
            Some(FeedbackNoteKind::Note),
            FeedbackProvenance::new(None, Some("spec-api/system".to_string()), None)?,
        )?;
        let _ = store.record_entry(entry)?;
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::parse_guards_from_markdown;

    #[test]
    fn parses_guard_ids_from_markdown_list() {
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
}
