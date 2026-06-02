use std::path::PathBuf;

use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocEvidenceKind {
    AuthoredDocCheck,
    GeneratedGuidanceCheck,
    ManualVerificationStep,
    CoverageGap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocEvidenceStatus {
    Satisfied,
    Missing,
    Blocked,
}

impl DocEvidenceStatus {
    pub fn is_satisfied(&self) -> bool {
        matches!(self, Self::Satisfied)
    }

    pub fn is_blocking(&self) -> bool {
        !self.is_satisfied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocEvidenceLinks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spec_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance_criterion_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ticket_ids: Vec<String>,
}

impl DocEvidenceLinks {
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocEvidenceRecord {
    pub id: String,
    pub title: String,
    pub kind: DocEvidenceKind,
    pub status: DocEvidenceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_paths: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_rule_ids: Vec<String>,
    #[serde(default)]
    pub links: DocEvidenceLinks,
}

impl DocEvidenceRecord {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        kind: DocEvidenceKind,
        status: DocEvidenceStatus,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            kind,
            status,
            detail: None,
            document_paths: Vec::new(),
            related_rule_ids: Vec::new(),
            links: DocEvidenceLinks::default(),
        }
    }

    pub fn is_satisfied(&self) -> bool {
        self.status.is_satisfied()
    }

    pub fn is_blocking(&self) -> bool {
        self.status.is_blocking()
    }

    pub fn satisfies_acceptance(
        &self,
        acceptance_criterion_id: &str,
    ) -> bool {
        self.is_satisfied()
            && self.links.links_to_acceptance(acceptance_criterion_id)
    }

    pub fn blocks_acceptance(
        &self,
        acceptance_criterion_id: &str,
    ) -> bool {
        self.is_blocking()
            && self.links.links_to_acceptance(acceptance_criterion_id)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{
        DocEvidenceKind,
        DocEvidenceLinks,
        DocEvidenceRecord,
        DocEvidenceStatus,
    };

    #[test]
    fn serde_round_trips_supported_evidence_record_kinds() {
        let records = vec![
            DocEvidenceRecord {
                id: "doc-authored".to_string(),
                title: "Author README coverage".to_string(),
                kind: DocEvidenceKind::AuthoredDocCheck,
                status: DocEvidenceStatus::Satisfied,
                detail: Some("README updated with rollout notes".to_string()),
                document_paths: vec!["README.md".into()],
                related_rule_ids: vec!["rule-generated-guidance".to_string()],
                links: DocEvidenceLinks {
                    spec_ids: vec!["spec-docs".to_string()],
                    acceptance_criterion_ids: vec!["criterion-docs".to_string()],
                    ticket_ids: vec!["ticket-docs".to_string()],
                },
            },
            DocEvidenceRecord::new(
                "doc-generated",
                "Generated guidance check",
                DocEvidenceKind::GeneratedGuidanceCheck,
                DocEvidenceStatus::Satisfied,
            ),
            DocEvidenceRecord::new(
                "doc-manual",
                "Manual review step",
                DocEvidenceKind::ManualVerificationStep,
                DocEvidenceStatus::Blocked,
            ),
            DocEvidenceRecord::new(
                "doc-gap",
                "Coverage gap",
                DocEvidenceKind::CoverageGap,
                DocEvidenceStatus::Missing,
            ),
        ];

        let json = serde_json::to_string_pretty(&records).unwrap();
        let reparsed: Vec<DocEvidenceRecord> = serde_json::from_str(&json).unwrap();

        assert_eq!(reparsed, records);
        assert!(json.contains("authored-doc-check"));
        assert!(json.contains("generated-guidance-check"));
        assert!(json.contains("manual-verification-step"));
        assert!(json.contains("coverage-gap"));
    }

    #[test]
    fn evidence_links_report_targeted_spec_acceptance_and_ticket_ids() {
        let links = DocEvidenceLinks {
            spec_ids: vec!["spec-a".to_string()],
            acceptance_criterion_ids: vec!["criterion-a".to_string()],
            ticket_ids: vec!["ticket-a".to_string()],
        };

        assert!(links.links_to_spec("spec-a"));
        assert!(links.links_to_acceptance("criterion-a"));
        assert!(links.links_to_ticket("ticket-a"));
        assert!(!links.links_to_spec("spec-b"));
        assert!(!links.links_to_acceptance("criterion-b"));
        assert!(!links.links_to_ticket("ticket-b"));
    }

    #[test]
    fn coverage_gaps_and_manual_steps_can_block_or_satisfy_acceptance() {
        let blocking_gap = DocEvidenceRecord {
            id: "doc-gap".to_string(),
            title: "Coverage gap".to_string(),
            kind: DocEvidenceKind::CoverageGap,
            status: DocEvidenceStatus::Missing,
            detail: Some("No generated guidance check exists yet".to_string()),
            document_paths: Vec::new(),
            related_rule_ids: vec!["guidance-rule".to_string()],
            links: DocEvidenceLinks {
                spec_ids: vec!["spec-a".to_string()],
                acceptance_criterion_ids: vec!["criterion-a".to_string()],
                ticket_ids: vec!["ticket-a".to_string()],
            },
        };

        let satisfied_manual = DocEvidenceRecord {
            id: "doc-manual".to_string(),
            title: "Manual verification".to_string(),
            kind: DocEvidenceKind::ManualVerificationStep,
            status: DocEvidenceStatus::Satisfied,
            detail: Some("Reviewer confirmed coverage in generated README".to_string()),
            document_paths: vec!["README.md".into()],
            related_rule_ids: Vec::new(),
            links: DocEvidenceLinks {
                spec_ids: vec!["spec-a".to_string()],
                acceptance_criterion_ids: vec!["criterion-a".to_string()],
                ticket_ids: vec!["ticket-a".to_string()],
            },
        };

        assert!(blocking_gap.is_blocking());
        assert!(blocking_gap.blocks_acceptance("criterion-a"));
        assert!(!blocking_gap.satisfies_acceptance("criterion-a"));

        assert!(satisfied_manual.is_satisfied());
        assert!(satisfied_manual.satisfies_acceptance("criterion-a"));
        assert!(!satisfied_manual.blocks_acceptance("criterion-a"));
    }
}