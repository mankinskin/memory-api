use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

mod benchmark;
mod error;
mod interoperability;
mod store;
mod store_index;

pub use benchmark::{
    BenchmarkExecution,
    BenchmarkQuery,
    BudgetTable,
    ingest_criterion_estimates,
};
pub use error::TestError;
pub use interoperability::{
    IdentifiableArtifact,
    InteroperableArtifact,
    TraceableArtifact,
};
pub use store::{
    ExecutionQuery,
    TestStoreConfig,
};
pub use store_index::{
    BenchmarkGroupSummary,
    IssueEntry,
    SlowEntry,
    TEST_INDEX_FILE_COMMENT,
    TestStoreIndexArtifacts,
    TestStoreIndexInput,
    TestStoreSummary,
    ValidationGroupSummary,
    generate_test_store_index,
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
    pub fn has_traceability_links(&self) -> bool {
        !self.spec_ids.is_empty()
            || !self.acceptance_criterion_ids.is_empty()
            || !self.ticket_ids.is_empty()
    }

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

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ValidationProvenance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

impl ValidationProvenance {
    pub fn is_empty(&self) -> bool {
        self.source_path.is_none()
            && self.test_id.is_none()
            && self.domain.is_none()
            && self.operation.is_none()
            && self.transport.is_none()
            && self.run_id.is_none()
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slow_threshold_ms: Option<u64>,
    #[serde(default)]
    pub links: ValidationLinks,
    #[serde(default, skip_serializing_if = "ValidationProvenance::is_empty")]
    pub provenance: ValidationProvenance,
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
            slow_threshold_ms: None,
            links: ValidationLinks::default(),
            provenance: ValidationProvenance::default(),
        }
    }

    pub fn targets_acceptance(
        &self,
        acceptance_criterion_id: &str,
    ) -> bool {
        self.links.links_to_acceptance(acceptance_criterion_id)
    }

    pub fn is_over_budget(
        &self,
        execution: &ValidationExecution,
    ) -> bool {
        match (self.slow_threshold_ms, execution.duration_ms) {
            (Some(threshold), Some(duration)) => duration > threshold,
            _ => false,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionSort {
    #[default]
    NewestFirst,
    SlowestFirst,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationExecution {
    pub id: String,
    pub validation_spec_id: String,
    pub outcome: ValidationOutcome,
    pub executed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throughput: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default)]
    pub links: ValidationLinks,
    #[serde(default, skip_serializing_if = "ValidationProvenance::is_empty")]
    pub provenance: ValidationProvenance,
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
            duration_ms: None,
            throughput: None,
            detail: None,
            links: ValidationLinks::default(),
            provenance: ValidationProvenance::default(),
        }
    }

    pub fn passed(
        id: impl Into<String>,
        validation_spec_id: impl Into<String>,
        executed_at: DateTime<Utc>,
    ) -> Self {
        Self::new(
            id,
            validation_spec_id,
            ValidationOutcome::Passed,
            executed_at,
        )
    }

    pub fn failed(
        id: impl Into<String>,
        validation_spec_id: impl Into<String>,
        executed_at: DateTime<Utc>,
    ) -> Self {
        Self::new(
            id,
            validation_spec_id,
            ValidationOutcome::Failed,
            executed_at,
        )
    }

    pub fn blocked(
        id: impl Into<String>,
        validation_spec_id: impl Into<String>,
        executed_at: DateTime<Utc>,
    ) -> Self {
        Self::new(
            id,
            validation_spec_id,
            ValidationOutcome::Blocked,
            executed_at,
        )
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

impl IdentifiableArtifact for ValidationExecution {
    type Id = str;
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

impl InteroperableArtifact for ValidationExecution {
    fn artifact_class(&self) -> &'static str {
        "validation-execution"
    }

    fn interoperability_gaps(&self) -> Vec<&'static str> {
        let mut gaps = Vec::new();
        if self.provenance.domain.as_deref().is_none() {
            gaps.push("missing provenance.domain");
        }
        if self.provenance.operation.as_deref().is_none() {
            gaps.push("missing provenance.operation");
        }
        if self.provenance.run_id.as_deref().is_none() {
            gaps.push("missing provenance.run_id");
        }
        if !self.links.has_traceability_links() {
            gaps.push("missing spec, acceptance, or ticket links");
        }
        gaps
    }
}

impl TraceableArtifact for ValidationExecution {
    fn domain(&self) -> Option<&str> {
        self.provenance.domain.as_deref()
    }
    fn operation(&self) -> Option<&str> {
        self.provenance.operation.as_deref()
    }
    fn run_id(&self) -> Option<&str> {
        self.provenance.run_id.as_deref()
    }
    fn has_traceability_links(&self) -> bool {
        self.links.has_traceability_links()
    }
}

impl ValidationExecution {
    pub fn interoperability_gaps(&self) -> Vec<&'static str> {
        <Self as InteroperableArtifact>::interoperability_gaps(self)
    }

    pub fn validate_interoperability_contract(&self) -> Result<(), TestError> {
        let gaps = self.interoperability_gaps();
        if gaps.is_empty() {
            return Ok(());
        }

        Err(TestError::InteroperabilityContract {
            record_kind: <Self as InteroperableArtifact>::artifact_class(self)
                .to_string(),
            detail: gaps.join(", "),
        })
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
