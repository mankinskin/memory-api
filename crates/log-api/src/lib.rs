use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use test_api::{
    ValidationExecution,
    ValidationLinks,
};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ValidationLogLinks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spec_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance_criterion_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ticket_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doc_evidence_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_execution_ids: Vec<String>,
}

impl ValidationLogLinks {
    pub fn links_to_spec(
        &self,
        spec_id: &str,
    ) -> bool {
        self.spec_ids.iter().any(|id| id == spec_id)
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

    pub fn links_to_execution(
        &self,
        execution_id: &str,
    ) -> bool {
        self.validation_execution_ids
            .iter()
            .any(|id| id == execution_id)
    }
}

impl From<&ValidationExecution> for ValidationLogLinks {
    fn from(execution: &ValidationExecution) -> Self {
        Self {
            spec_ids: execution.links.spec_ids.clone(),
            acceptance_criterion_ids: execution
                .links
                .acceptance_criterion_ids
                .clone(),
            ticket_ids: execution.links.ticket_ids.clone(),
            doc_evidence_ids: execution.links.doc_evidence_ids.clone(),
            validation_execution_ids: vec![execution.id.clone()],
        }
    }
}

impl From<ValidationLinks> for ValidationLogLinks {
    fn from(links: ValidationLinks) -> Self {
        Self {
            spec_ids: links.spec_ids,
            acceptance_criterion_ids: links.acceptance_criterion_ids,
            ticket_ids: links.ticket_ids,
            doc_evidence_ids: links.doc_evidence_ids,
            validation_execution_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationLogKind {
    Stdout,
    Stderr,
    CombinedOutput,
    StructuredSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationLogCapture {
    pub id: String,
    pub validation_execution_id: String,
    pub kind: ValidationLogKind,
    pub captured_at: DateTime<Utc>,
    pub media_type: String,
    pub locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default)]
    pub links: ValidationLogLinks,
}

impl ValidationLogCapture {
    pub fn from_execution(
        id: impl Into<String>,
        execution: &ValidationExecution,
        kind: ValidationLogKind,
        captured_at: DateTime<Utc>,
        media_type: impl Into<String>,
        locator: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            validation_execution_id: execution.id.clone(),
            kind,
            captured_at,
            media_type: media_type.into(),
            locator: locator.into(),
            detail: None,
            links: ValidationLogLinks::from(execution),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationLogRetrieval {
    pub id: String,
    pub capture_id: String,
    pub requested_at: DateTime<Utc>,
    pub locator: String,
    pub media_type: String,
    #[serde(default)]
    pub links: ValidationLogLinks,
}

impl ValidationLogRetrieval {
    pub fn new(
        id: impl Into<String>,
        capture_id: impl Into<String>,
        requested_at: DateTime<Utc>,
        locator: impl Into<String>,
        media_type: impl Into<String>,
        links: ValidationLogLinks,
    ) -> Self {
        Self {
            id: id.into(),
            capture_id: capture_id.into(),
            requested_at,
            locator: locator.into(),
            media_type: media_type.into(),
            links,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;
    use test_api::{
        ValidationExecution,
        ValidationLinks,
    };

    use super::{
        ValidationLogCapture,
        ValidationLogKind,
        ValidationLogLinks,
        ValidationLogRetrieval,
    };

    fn sample_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 2, 12, 30, 0)
            .single()
            .unwrap()
    }

    fn sample_execution() -> ValidationExecution {
        let mut execution = ValidationExecution::passed(
            "exec-1",
            "validation-spec-1",
            sample_time(),
        );
        execution.links = ValidationLinks {
            spec_ids: vec!["spec-1".to_string()],
            acceptance_criterion_ids: vec!["criterion-1".to_string()],
            ticket_ids: vec!["ticket-1".to_string()],
            doc_evidence_ids: vec!["doc-1".to_string()],
            log_ids: vec!["future-log".to_string()],
        };
        execution
    }

    #[test]
    fn captures_and_retrievals_round_trip_through_serde() {
        let execution = sample_execution();
        let capture = ValidationLogCapture::from_execution(
            "capture-1",
            &execution,
            ValidationLogKind::CombinedOutput,
            sample_time(),
            "text/plain",
            "target/test-logs/spec.log",
        );
        let retrieval = ValidationLogRetrieval::new(
            "retrieval-1",
            capture.id.clone(),
            sample_time(),
            capture.locator.clone(),
            capture.media_type.clone(),
            capture.links.clone(),
        );

        let json = serde_json::to_string_pretty(&(capture.clone(), retrieval.clone())).unwrap();
        let reparsed: (ValidationLogCapture, ValidationLogRetrieval) = serde_json::from_str(&json).unwrap();

        assert_eq!(reparsed.0, capture);
        assert_eq!(reparsed.1, retrieval);
        assert!(json.contains("combined-output"));
    }

    #[test]
    fn captures_inherit_execution_links_and_identity() {
        let execution = sample_execution();
        let capture = ValidationLogCapture::from_execution(
            "capture-1",
            &execution,
            ValidationLogKind::Stdout,
            sample_time(),
            "text/plain",
            "target/test-logs/spec.stdout",
        );

        assert_eq!(capture.validation_execution_id, execution.id);
        assert!(capture.links.links_to_execution("exec-1"));
        assert!(capture.links.links_to_spec("spec-1"));
        assert!(capture.links.links_to_ticket("ticket-1"));
        assert!(capture.links.links_to_doc_evidence("doc-1"));
    }

    #[test]
    fn retrievals_preserve_locator_and_link_metadata() {
        let links = ValidationLogLinks {
            spec_ids: vec!["spec-1".to_string()],
            acceptance_criterion_ids: vec!["criterion-1".to_string()],
            ticket_ids: vec!["ticket-1".to_string()],
            doc_evidence_ids: vec!["doc-1".to_string()],
            validation_execution_ids: vec!["exec-1".to_string()],
        };

        let retrieval = ValidationLogRetrieval::new(
            "retrieval-1",
            "capture-1",
            sample_time(),
            "target/test-logs/spec.stderr",
            "text/plain",
            links,
        );

        assert_eq!(retrieval.locator, "target/test-logs/spec.stderr");
        assert!(retrieval.links.links_to_spec("spec-1"));
        assert!(retrieval.links.links_to_ticket("ticket-1"));
        assert!(retrieval.links.links_to_doc_evidence("doc-1"));
        assert!(retrieval.links.links_to_execution("exec-1"));
    }
}