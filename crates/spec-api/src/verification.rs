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
            let heading =
                trimmed.trim_start_matches('#').trim().to_ascii_lowercase();
            in_guards = heading == "guards";
            continue;
        }

        if in_guards && (trimmed.starts_with('-') || trimmed.starts_with('*')) {
            if let Some(start) = trimmed.find('`') {
                if let Some(end) = trimmed[start + 1..].find('`') {
                    guards.push(
                        trimmed[start + 1..start + 1 + end].trim().to_string(),
                    );
                }
            }
        }
    }
    guards
}

/// Distinct outcomes of a spec verified-state recomputation.
///
/// Previously these were all collapsed into `Ok(false)`, which made it
/// impossible for a caller to distinguish "not verifiable yet" from
/// "verification failed". Each variant is independently actionable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum SpecVerificationOutcome {
    /// The spec declares no guards, so verified state is not applicable.
    NoGuards,
    /// One or more declared guards have no recorded execution yet.
    Pending { missing_guards: Vec<String> },
    /// Every declared guard has executed, but at least one did not pass.
    Failed { failed_guards: Vec<String> },
    /// Every declared guard passed; the spec was transitioned to `verified`.
    Verified,
}

impl SpecVerificationOutcome {
    /// Whether this outcome means the spec is now `verified`.
    pub fn is_verified(&self) -> bool {
        matches!(self, Self::Verified)
    }

    /// A short, machine-stable label for logging and JSON summaries.
    pub fn label(&self) -> &'static str {
        match self {
            Self::NoGuards => "no-guards",
            Self::Pending { .. } => "pending",
            Self::Failed { .. } => "failed",
            Self::Verified => "verified",
        }
    }
}

/// Recompute spec `verified` state from guard execution outcomes.
///
/// Returns a [`SpecVerificationOutcome`] describing exactly why the spec is or
/// is not verified. The `Err` channel is reserved for genuine failures (spec
/// not found, store IO, feedback recording) — never for "not verified".
pub fn recompute_spec_verified_state(
    spec_store: &mut SpecStore,
    test_store: &TestStoreConfig,
    feedback_store: Option<&EntityFeedbackStore>,
    spec_id_or_slug: &str,
) -> Result<SpecVerificationOutcome, String> {
    let (_spec, body) = spec_store
        .get_full(spec_id_or_slug)
        .map_err(|e| format!("Spec not found: {e}"))?;

    let guards = parse_guards_from_markdown(&body);
    if guards.is_empty() {
        return Ok(SpecVerificationOutcome::NoGuards);
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

    let missing_guards: Vec<String> = guards
        .iter()
        .filter(|guard| !latest_executions.contains_key(*guard))
        .cloned()
        .collect();
    if !missing_guards.is_empty() {
        return Ok(SpecVerificationOutcome::Pending { missing_guards });
    }

    let failed_guards: Vec<String> = guards
        .iter()
        .filter(|guard| {
            !latest_executions.get(*guard).is_some_and(|exec| {
                matches!(exec.outcome, ValidationOutcome::Passed)
            })
        })
        .cloned()
        .collect();
    if !failed_guards.is_empty() {
        return Ok(SpecVerificationOutcome::Failed { failed_guards });
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
            Some(
                "spec guards passed and verified state recomputed".to_string(),
            ),
            Some(FeedbackNoteKind::Note),
            FeedbackProvenance::new(
                None,
                Some("spec-api/system".to_string()),
                None,
            )?,
        )?;
        let _ = store.record_entry(entry)?;
    }

    Ok(SpecVerificationOutcome::Verified)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use test_api::TestStoreConfig;

    use super::{
        SpecVerificationOutcome,
        parse_guards_from_markdown,
        recompute_spec_verified_state,
    };
    use crate::{
        SpecManifest,
        SpecStore,
    };

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

    fn spec_store_with_body(body: &str) -> (TempDir, SpecStore, String) {
        let tmp = TempDir::new().expect("tempdir");
        let mut store = SpecStore::init(tmp.path()).expect("init spec store");
        let manifest =
            SpecManifest::new("root/guarded", "Guarded", "test-component");
        let id = store.create(&manifest, body, None).expect("create spec");
        (tmp, store, id.to_string())
    }

    fn empty_test_store() -> (TempDir, TestStoreConfig) {
        let tmp = TempDir::new().expect("test tempdir");
        let config = TestStoreConfig::new(tmp.path(), "memory-api");
        (tmp, config)
    }

    #[test]
    fn recompute_reports_no_guards_when_spec_declares_none() {
        let (_spec_tmp, mut store, id) =
            spec_store_with_body("# Spec\n\nNo guards declared here.\n");
        let (_test_tmp, test_store) = empty_test_store();

        let outcome =
            recompute_spec_verified_state(&mut store, &test_store, None, &id)
                .expect("recompute");

        assert_eq!(outcome, SpecVerificationOutcome::NoGuards);
        assert!(!outcome.is_verified());
        assert_eq!(outcome.label(), "no-guards");
    }

    #[test]
    fn recompute_reports_pending_with_missing_guard_ids() {
        let body = "# Spec\n\n## Guards\n- `val-alpha`\n- `val-beta`\n";
        let (_spec_tmp, mut store, id) = spec_store_with_body(body);
        let (_test_tmp, test_store) = empty_test_store();

        let outcome =
            recompute_spec_verified_state(&mut store, &test_store, None, &id)
                .expect("recompute");

        match outcome {
            SpecVerificationOutcome::Pending { missing_guards } => {
                assert_eq!(missing_guards, vec!["val-alpha", "val-beta"]);
            },
            other => panic!("expected pending outcome, got {other:?}"),
        }
    }
}
