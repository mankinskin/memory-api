use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ValidationLinks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spec_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance_criterion_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ticket_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doc_evidence_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log_ids: Vec<String>,
}

impl ValidationLinks {
    pub fn links_to_spec(
        &self,
        spec_id: &str,
    ) -> bool {
        self.spec_ids.iter().any(|id| id == spec_id)
    }

    pub fn links_to_acceptance(
        &self,
        acceptance_criterion_id: &str,
    ) -> bool {
        self.acceptance_criterion_ids
            .iter()
            .any(|id| id == acceptance_criterion_id)
    }

    pub fn links_to_ticket(
        &self,
        ticket_id: &str,
    ) -> bool {
        self.ticket_ids.iter().any(|id| id == ticket_id)
    }

    pub fn links_to_doc_evidence(
        &self,
        doc_evidence_id: &str,
    ) -> bool {
        self.doc_evidence_ids.iter().any(|id| id == doc_evidence_id)
    }

    pub fn links_to_log(
        &self,
        log_id: &str,
    ) -> bool {
        self.log_ids.iter().any(|id| id == log_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationSpec {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default)]
    pub links: ValidationLinks,
}

impl ValidationSpec {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            command: None,
            detail: None,
            links: ValidationLinks::default(),
        }
    }

    pub fn targets_acceptance(
        &self,
        acceptance_criterion_id: &str,
    ) -> bool {
        self.links.links_to_acceptance(acceptance_criterion_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationOutcome {
    Passed,
    Failed,
    Blocked,
}

impl ValidationOutcome {
    pub fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed)
    }

    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationExecution {
    pub id: String,
    pub validation_spec_id: String,
    pub outcome: ValidationOutcome,
    pub executed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default)]
    pub links: ValidationLinks,
}

impl ValidationExecution {
    pub fn new(
        id: impl Into<String>,
        validation_spec_id: impl Into<String>,
        outcome: ValidationOutcome,
        executed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: id.into(),
            validation_spec_id: validation_spec_id.into(),
            outcome,
            executed_at,
            detail: None,
            links: ValidationLinks::default(),
        }
    }

    pub fn passed(
        id: impl Into<String>,
        validation_spec_id: impl Into<String>,
        executed_at: DateTime<Utc>,
    ) -> Self {
        Self::new(id, validation_spec_id, ValidationOutcome::Passed, executed_at)
    }

    pub fn failed(
        id: impl Into<String>,
        validation_spec_id: impl Into<String>,
        executed_at: DateTime<Utc>,
    ) -> Self {
        Self::new(id, validation_spec_id, ValidationOutcome::Failed, executed_at)
    }

    pub fn blocked(
        id: impl Into<String>,
        validation_spec_id: impl Into<String>,
        executed_at: DateTime<Utc>,
    ) -> Self {
        Self::new(id, validation_spec_id, ValidationOutcome::Blocked, executed_at)
    }

    pub fn references_doc_evidence(
        &self,
        doc_evidence_id: &str,
    ) -> bool {
        self.links.links_to_doc_evidence(doc_evidence_id)
    }

    pub fn references_log(
        &self,
        log_id: &str,
    ) -> bool {
        self.links.links_to_log(log_id)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;

    use super::{
        ValidationExecution,
        ValidationLinks,
        ValidationOutcome,
        ValidationSpec,
    };

    fn sample_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 2, 12, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn validation_entities_round_trip_through_serde() {
        let spec = ValidationSpec {
            id: "validation-spec-1".to_string(),
            title: "Spec health check".to_string(),
            command: Some("cargo test -p spec-api contract -- --nocapture".to_string()),
            detail: Some("Covers expectation-oriented contract health".to_string()),
            links: ValidationLinks {
                spec_ids: vec!["spec-api/contract".to_string()],
                acceptance_criterion_ids: vec!["criterion-contract-health".to_string()],
                ticket_ids: vec!["ticket-contract-health".to_string()],
                doc_evidence_ids: vec!["doc-evidence-1".to_string()],
                log_ids: vec!["log-1".to_string()],
            },
        };
        let execution = ValidationExecution {
            id: "validation-exec-1".to_string(),
            validation_spec_id: spec.id.clone(),
            outcome: ValidationOutcome::Passed,
            executed_at: sample_time(),
            detail: Some("Contract tests passed against structured fields".to_string()),
            links: spec.links.clone(),
        };

        let json = serde_json::to_string_pretty(&(spec.clone(), execution.clone())).unwrap();
        let reparsed: (ValidationSpec, ValidationExecution) = serde_json::from_str(&json).unwrap();

        assert_eq!(reparsed.0, spec);
        assert_eq!(reparsed.1, execution);
        assert!(json.contains("passed"));
    }

    #[test]
    fn execution_helpers_cover_passed_failed_and_blocked_outcomes() {
        let passed = ValidationExecution::passed("exec-pass", "spec-a", sample_time());
        let failed = ValidationExecution::failed("exec-fail", "spec-a", sample_time());
        let blocked = ValidationExecution::blocked("exec-blocked", "spec-a", sample_time());

        assert!(passed.outcome.is_passed());
        assert!(failed.outcome.is_failed());
        assert!(blocked.outcome.is_blocked());
    }

    #[test]
    fn links_connect_specs_tickets_doc_evidence_and_future_logs() {
        let mut spec = ValidationSpec::new("validation-spec-1", "Guidance validation");
        spec.links = ValidationLinks {
            spec_ids: vec!["spec-guidance".to_string()],
            acceptance_criterion_ids: vec!["criterion-guidance".to_string()],
            ticket_ids: vec!["ticket-guidance".to_string()],
            doc_evidence_ids: vec!["doc-guidance".to_string()],
            log_ids: vec!["log-guidance".to_string()],
        };

        let execution = ValidationExecution {
            id: "exec-guidance".to_string(),
            validation_spec_id: spec.id.clone(),
            outcome: ValidationOutcome::Blocked,
            executed_at: sample_time(),
            detail: Some("Blocked by missing generated guidance output".to_string()),
            links: spec.links.clone(),
        };

        assert!(spec.targets_acceptance("criterion-guidance"));
        assert!(spec.links.links_to_spec("spec-guidance"));
        assert!(spec.links.links_to_ticket("ticket-guidance"));
        assert!(execution.references_doc_evidence("doc-guidance"));
        assert!(execution.references_log("log-guidance"));
        assert!(!execution.references_doc_evidence("doc-other"));
    }
}