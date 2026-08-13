use chrono::{
    DateTime,
    Utc,
};
use memory_kernel::InteroperableArtifact;
use serde::{
    Deserialize,
    Serialize,
};
use test_api::{
    ValidationExecution,
    ValidationLinks,
    IdentifiableArtifact,
    TraceableArtifact,
};

mod error;
mod store;

pub use error::LogError;
pub use store::{
    LogCaptureQuery,
    LogStoreConfig,
    RuntimeLogSessionQuery,
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

impl IdentifiableArtifact for ValidationLogCapture {
    type Id = str;
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

impl InteroperableArtifact for ValidationLogCapture {
    fn artifact_class(&self) -> &'static str {
        "validation-log-capture"
    }

    fn interoperability_gaps(&self) -> Vec<&'static str> {
        let mut gaps = Vec::new();
        if self.validation_execution_id.trim().is_empty() {
            gaps.push("missing validation_execution_id");
        }
        if !self.links.links_to_execution(&self.validation_execution_id) {
            gaps.push("missing execution link");
        }
        gaps
    }
}

impl ValidationLogCapture {
    pub fn interoperability_gaps(&self) -> Vec<&'static str> {
        <Self as InteroperableArtifact>::interoperability_gaps(self)
    }

    pub fn validate_interoperability_contract(
        &self
    ) -> Result<(), crate::LogError> {
        let gaps = self.interoperability_gaps();
        if gaps.is_empty() {
            return Ok(());
        }

        Err(crate::LogError::InteroperabilityContract {
            record_kind: <Self as InteroperableArtifact>::artifact_class(self).to_string(),
            detail: gaps.join(", "),
        })
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

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RuntimeLogLinks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spec_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ticket_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doc_evidence_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_execution_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub benchmark_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_session_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub journal_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graph_operation_ids: Vec<String>,
}

impl RuntimeLogLinks {
    pub fn has_correlation_links(&self) -> bool {
        !self.validation_execution_ids.is_empty()
            || !self.benchmark_ids.is_empty()
            || !self.agent_session_ids.is_empty()
            || !self.journal_ids.is_empty()
            || !self.graph_operation_ids.is_empty()
    }

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

    pub fn links_to_execution(
        &self,
        execution_id: &str,
    ) -> bool {
        self.validation_execution_ids
            .iter()
            .any(|id| id == execution_id)
    }

    pub fn links_to_benchmark(
        &self,
        benchmark_id: &str,
    ) -> bool {
        self.benchmark_ids.iter().any(|id| id == benchmark_id)
    }

    pub fn links_to_agent_session(
        &self,
        session_id: &str,
    ) -> bool {
        self.agent_session_ids.iter().any(|id| id == session_id)
    }

    pub fn links_to_journal(
        &self,
        journal_id: &str,
    ) -> bool {
        self.journal_ids.iter().any(|id| id == journal_id)
    }

    pub fn links_to_graph_operation(
        &self,
        graph_operation_id: &str,
    ) -> bool {
        self.graph_operation_ids
            .iter()
            .any(|id| id == graph_operation_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeLogTransport {
    Cli,
    Mcp,
    Http,
    Test,
    Bench,
    AgentSession,
    InProcess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeLogStatus {
    Active,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeLogFormat {
    JsonLines,
    PlainText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeLogSession {
    pub id: String,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    pub status: RuntimeLogStatus,
    pub component: String,
    pub transport: RuntimeLogTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_root: Option<String>,
    pub locator: String,
    pub media_type: String,
    pub format: RuntimeLogFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_filters: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_offset_checkpoint: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default)]
    pub links: RuntimeLogLinks,
}

impl IdentifiableArtifact for RuntimeLogSession {
    type Id = str;
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

impl InteroperableArtifact for RuntimeLogSession {
    fn artifact_class(&self) -> &'static str {
        "runtime-log-session"
    }

    fn interoperability_gaps(&self) -> Vec<&'static str> {
        let mut gaps = Vec::new();
        if self.operation.as_deref().is_none() {
            gaps.push("missing operation");
        }
        if self.run_id.as_deref().is_none() {
            gaps.push("missing run_id");
        }
        if !self.links.has_correlation_links() {
            gaps.push("missing execution, benchmark, journal, agent-session, or graph-operation links");
        }
        gaps
    }
}

impl TraceableArtifact for RuntimeLogSession {
    fn domain(&self) -> Option<&str> {
        // RuntimeLogSession carries no explicit domain field.
        None
    }
    fn operation(&self) -> Option<&str> {
        self.operation.as_deref()
    }
    fn run_id(&self) -> Option<&str> {
        self.run_id.as_deref()
    }
    fn has_traceability_links(&self) -> bool {
        self.links.has_correlation_links()
    }
}

impl RuntimeLogSession {
    pub fn interoperability_gaps(&self) -> Vec<&'static str> {
        <Self as InteroperableArtifact>::interoperability_gaps(self)
    }

    pub fn validate_interoperability_contract(
        &self
    ) -> Result<(), crate::LogError> {
        let gaps = self.interoperability_gaps();
        if gaps.is_empty() {
            return Ok(());
        }

        Err(crate::LogError::InteroperabilityContract {
            record_kind: <Self as InteroperableArtifact>::artifact_class(self).to_string(),
            detail: gaps.join(", "),
        })
    }
}

impl RuntimeLogSession {
    pub fn new(
        id: impl Into<String>,
        started_at: DateTime<Utc>,
        status: RuntimeLogStatus,
        component: impl Into<String>,
        transport: RuntimeLogTransport,
        locator: impl Into<String>,
        media_type: impl Into<String>,
        format: RuntimeLogFormat,
    ) -> Self {
        Self {
            id: id.into(),
            started_at,
            ended_at: None,
            status,
            component: component.into(),
            transport,
            operation: None,
            tool: None,
            route: None,
            run_id: None,
            process_id: None,
            workspace_root: None,
            store_root: None,
            locator: locator.into(),
            media_type: media_type.into(),
            format,
            rotation_policy: None,
            active_filters: Vec::new(),
            byte_offset_checkpoint: None,
            detail: None,
            links: RuntimeLogLinks::default(),
        }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
