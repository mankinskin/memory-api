use std::collections::BTreeMap;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
    de::DeserializeOwned,
};
use serde_json::Value;
use uuid::Uuid;

use crate::code_ref::CodeRef;

pub type SpecId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SpecContractMode {
    ExpectationOriented,
}

impl SpecContractMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExpectationOriented => "expectation-oriented",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExpectedProperty {
    pub id: String,
    pub statement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptanceCriterion {
    pub id: String,
    pub statement: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_property_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceRequirement {
    pub id: String,
    pub kind: String,
    pub description: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FulfillmentSubjectKind {
    AcceptanceCriterion,
    EvidenceRequirement,
}

impl FulfillmentSubjectKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AcceptanceCriterion => "acceptance-criterion",
            Self::EvidenceRequirement => "evidence-requirement",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FulfillmentStatus {
    Pending,
    Satisfied,
    Blocked,
}

impl FulfillmentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Satisfied => "satisfied",
            Self::Blocked => "blocked",
        }
    }

    pub fn is_satisfied(&self) -> bool {
        matches!(self, Self::Satisfied)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FulfillmentSummary {
    pub id: String,
    pub subject_kind: FulfillmentSubjectKind,
    pub subject_id: String,
    pub status: FulfillmentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpecHealthFinding {
    pub id: SpecId,
    pub issue: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SpecHealthReport {
    pub specs_checked: usize,
    pub issues: Vec<SpecHealthFinding>,
}

impl SpecHealthReport {
    pub fn issues_count(&self) -> usize {
        self.issues.len()
    }
}

/// A specification manifest — metadata about a spec stored in spec.toml.
///
/// Uses the same `extra: BTreeMap<String, Value>` storage pattern as
/// `EntityManifest` / `TicketManifest`. Spec-specific fields are stored in
/// the extra map and accessed via typed methods.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecManifest {
    pub id: SpecId,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_refs: Vec<CodeRef>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl SpecManifest {
    /// Create a new spec manifest with required fields.
    pub fn new(
        slug: &str,
        title: &str,
        component: &str,
    ) -> Self {
        let mut extra = BTreeMap::new();
        extra.insert("slug".to_string(), Value::String(slug.to_string()));
        extra.insert("title".to_string(), Value::String(title.to_string()));
        extra.insert(
            "component".to_string(),
            Value::String(component.to_string()),
        );
        extra.insert(
            "type".to_string(),
            Value::String("specification".to_string()),
        );
        extra.insert("state".to_string(), Value::String("draft".to_string()));

        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            code_refs: Vec::new(),
            extra,
        }
    }

    // ── typed accessors ──

    pub fn id(&self) -> SpecId {
        self.id
    }

    pub fn slug(&self) -> Option<&str> {
        self.extra.get("slug").and_then(|v| v.as_str())
    }

    pub fn title(&self) -> Option<&str> {
        self.extra.get("title").and_then(|v| v.as_str())
    }

    pub fn state(&self) -> Option<&str> {
        self.extra.get("state").and_then(|v| v.as_str())
    }

    pub fn component(&self) -> Option<&str> {
        self.extra.get("component").and_then(|v| v.as_str())
    }

    pub fn scope(&self) -> Option<&str> {
        self.extra.get("scope").and_then(|v| v.as_str())
    }

    pub fn parent(&self) -> Option<&str> {
        self.extra.get("parent").and_then(|v| v.as_str())
    }

    pub fn contract_mode(&self) -> Option<SpecContractMode> {
        self.parse_field("contract_mode").ok().flatten()
    }

    pub fn expected_properties(&self) -> Vec<ExpectedProperty> {
        self.parse_vec_field("expected_properties")
    }

    pub fn acceptance_criteria(&self) -> Vec<AcceptanceCriterion> {
        self.parse_vec_field("acceptance_criteria")
    }

    pub fn evidence_requirements(&self) -> Vec<EvidenceRequirement> {
        self.parse_vec_field("evidence_requirements")
    }

    pub fn fulfillment_summaries(&self) -> Vec<FulfillmentSummary> {
        self.parse_vec_field("fulfillment_summaries")
    }

    // ── setters ──

    pub fn set_slug(
        &mut self,
        slug: &str,
    ) {
        self.extra
            .insert("slug".to_string(), Value::String(slug.to_string()));
    }

    pub fn set_title(
        &mut self,
        title: &str,
    ) {
        self.extra
            .insert("title".to_string(), Value::String(title.to_string()));
    }

    pub fn set_state(
        &mut self,
        state: &str,
    ) {
        self.extra
            .insert("state".to_string(), Value::String(state.to_string()));
    }

    pub fn set_component(
        &mut self,
        comp: &str,
    ) {
        self.extra
            .insert("component".to_string(), Value::String(comp.to_string()));
    }

    pub fn set_scope(
        &mut self,
        scope: &str,
    ) {
        self.extra
            .insert("scope".to_string(), Value::String(scope.to_string()));
    }

    pub fn set_parent(
        &mut self,
        parent: &str,
    ) {
        self.extra
            .insert("parent".to_string(), Value::String(parent.to_string()));
    }

    pub fn set_contract_mode(
        &mut self,
        mode: Option<SpecContractMode>,
    ) {
        self.set_typed_field("contract_mode", mode);
    }

    pub fn set_expected_properties(
        &mut self,
        expected_properties: Vec<ExpectedProperty>,
    ) {
        self.set_typed_field("expected_properties", expected_properties);
    }

    pub fn set_acceptance_criteria(
        &mut self,
        acceptance_criteria: Vec<AcceptanceCriterion>,
    ) {
        self.set_typed_field("acceptance_criteria", acceptance_criteria);
    }

    pub fn set_evidence_requirements(
        &mut self,
        evidence_requirements: Vec<EvidenceRequirement>,
    ) {
        self.set_typed_field("evidence_requirements", evidence_requirements);
    }

    pub fn set_fulfillment_summaries(
        &mut self,
        fulfillment_summaries: Vec<FulfillmentSummary>,
    ) {
        self.set_typed_field("fulfillment_summaries", fulfillment_summaries);
    }

    /// Access the underlying extra fields.
    pub fn as_entity(&self) -> &BTreeMap<String, Value> {
        &self.extra
    }

    pub fn uses_structured_contract(&self) -> bool {
        self.extra.contains_key("contract_mode")
            || self.extra.contains_key("expected_properties")
            || self.extra.contains_key("acceptance_criteria")
            || self.extra.contains_key("evidence_requirements")
            || self.extra.contains_key("fulfillment_summaries")
    }

    pub fn contract_search_text(&self) -> String {
        let mut fragments = Vec::new();

        if let Some(mode) = self.contract_mode() {
            fragments.push(mode.as_str().to_string());
        }

        for property in self.expected_properties() {
            fragments.push(property.id);
            fragments.push(property.statement);
        }

        for criterion in self.acceptance_criteria() {
            fragments.push(criterion.id);
            fragments.push(criterion.statement);
            fragments.extend(criterion.expected_property_ids);
            fragments.extend(criterion.required_evidence_ids);
        }

        for evidence in self.evidence_requirements() {
            fragments.push(evidence.id);
            fragments.push(evidence.kind);
            fragments.push(evidence.description);
        }

        for summary in self.fulfillment_summaries() {
            fragments.push(summary.id);
            fragments.push(summary.subject_kind.as_str().to_string());
            fragments.push(summary.subject_id);
            fragments.push(summary.status.as_str().to_string());
            if let Some(detail) = summary.detail {
                fragments.push(detail);
            }
        }

        fragments
            .into_iter()
            .filter(|fragment| !fragment.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn health_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.slug().is_none() {
            issues.push("missing slug".to_string());
        }
        if self.title().is_none() {
            issues.push("missing title".to_string());
        }
        if self.component().is_none() {
            issues.push("missing component".to_string());
        }

        if !self.uses_structured_contract() {
            return issues;
        }

        match self.parse_field::<SpecContractMode>("contract_mode") {
            Ok(Some(_)) => {},
            Ok(None) => issues.push("missing contract mode".to_string()),
            Err(error) =>
                issues.push(format!("invalid contract mode: {error}")),
        }

        let expected_properties = match self
            .parse_field::<Vec<ExpectedProperty>>("expected_properties")
        {
            Ok(Some(properties)) => properties,
            Ok(None) => Vec::new(),
            Err(error) => {
                issues.push(format!("invalid expected properties: {error}"));
                Vec::new()
            },
        };
        let acceptance_criteria = match self
            .parse_field::<Vec<AcceptanceCriterion>>("acceptance_criteria")
        {
            Ok(Some(criteria)) => criteria,
            Ok(None) => Vec::new(),
            Err(error) => {
                issues.push(format!("invalid acceptance criteria: {error}"));
                Vec::new()
            },
        };
        let evidence_requirements = match self
            .parse_field::<Vec<EvidenceRequirement>>("evidence_requirements")
        {
            Ok(Some(evidence)) => evidence,
            Ok(None) => Vec::new(),
            Err(error) => {
                issues.push(format!("invalid evidence requirements: {error}"));
                Vec::new()
            },
        };
        let fulfillment_summaries = match self
            .parse_field::<Vec<FulfillmentSummary>>("fulfillment_summaries")
        {
            Ok(Some(summaries)) => summaries,
            Ok(None) => Vec::new(),
            Err(error) => {
                issues.push(format!("invalid fulfillment summaries: {error}"));
                Vec::new()
            },
        };

        if expected_properties.is_empty() {
            issues.push("missing expected properties".to_string());
        }
        if acceptance_criteria.is_empty() {
            issues.push("missing acceptance criteria".to_string());
        }
        if evidence_requirements.is_empty() {
            issues.push("missing evidence requirements".to_string());
        }

        let expected_property_ids = collect_unique_ids(
            &mut issues,
            expected_properties
                .iter()
                .map(|property| ("expected property", property.id.as_str())),
        );
        let acceptance_criterion_ids = collect_unique_ids(
            &mut issues,
            acceptance_criteria.iter().map(|criterion| {
                ("acceptance criterion", criterion.id.as_str())
            }),
        );
        let evidence_requirement_ids = collect_unique_ids(
            &mut issues,
            evidence_requirements
                .iter()
                .map(|evidence| ("evidence requirement", evidence.id.as_str())),
        );
        let _ = collect_unique_ids(
            &mut issues,
            fulfillment_summaries
                .iter()
                .map(|summary| ("fulfillment summary", summary.id.as_str())),
        );

        for criterion in &acceptance_criteria {
            if criterion.expected_property_ids.is_empty() {
                issues.push(format!(
                    "acceptance criterion '{}' missing expected property links",
                    criterion.id
                ));
            }
            if criterion.required_evidence_ids.is_empty() {
                issues.push(format!(
                    "acceptance criterion '{}' missing required evidence",
                    criterion.id
                ));
            }
            for property_id in &criterion.expected_property_ids {
                if !expected_property_ids.contains(property_id) {
                    issues.push(format!(
                        "acceptance criterion '{}' references missing expected property '{}'",
                        criterion.id, property_id
                    ));
                }
            }
            for evidence_id in &criterion.required_evidence_ids {
                if !evidence_requirement_ids.contains(evidence_id) {
                    issues.push(format!(
                        "acceptance criterion '{}' references missing evidence requirement '{}'",
                        criterion.id, evidence_id
                    ));
                }
            }
        }

        for summary in &fulfillment_summaries {
            let target_exists = match summary.subject_kind {
                FulfillmentSubjectKind::AcceptanceCriterion =>
                    acceptance_criterion_ids.contains(&summary.subject_id),
                FulfillmentSubjectKind::EvidenceRequirement =>
                    evidence_requirement_ids.contains(&summary.subject_id),
            };

            if !target_exists {
                issues.push(format!(
                    "fulfillment summary '{}' references missing {} '{}'",
                    summary.id,
                    summary.subject_kind.as_str(),
                    summary.subject_id
                ));
            }
        }

        for evidence in &evidence_requirements {
            if evidence.optional {
                continue;
            }

            let summaries: Vec<&FulfillmentSummary> = fulfillment_summaries
                .iter()
                .filter(|summary| {
                    summary.subject_kind
                        == FulfillmentSubjectKind::EvidenceRequirement
                        && summary.subject_id == evidence.id
                })
                .collect();

            if summaries.is_empty() {
                issues.push(format!(
                    "missing fulfillment summary for evidence requirement '{}'",
                    evidence.id
                ));
                continue;
            }

            if summaries
                .iter()
                .all(|summary| !summary.status.is_satisfied())
            {
                issues.push(format!(
                    "unsatisfied evidence requirement '{}'",
                    evidence.id
                ));
            }
        }

        issues
    }

    fn parse_field<T>(
        &self,
        key: &str,
    ) -> Result<Option<T>, String>
    where
        T: DeserializeOwned,
    {
        self.extra
            .get(key)
            .cloned()
            .map(|value| {
                serde_json::from_value(value).map_err(|error| error.to_string())
            })
            .transpose()
    }

    fn parse_vec_field<T>(
        &self,
        key: &str,
    ) -> Vec<T>
    where
        T: DeserializeOwned,
    {
        self.parse_field(key).ok().flatten().unwrap_or_default()
    }

    fn set_typed_field<T>(
        &mut self,
        key: &str,
        value: T,
    ) where
        T: Serialize,
    {
        match serde_json::to_value(value) {
            Ok(value) if should_remove_typed_field(&value) => {
                self.extra.remove(key);
            },
            Ok(value) => {
                self.extra.insert(key.to_string(), value);
            },
            Err(_) => {
                self.extra.remove(key);
            },
        }
    }
}

fn should_remove_typed_field(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Array(items) => items.is_empty(),
        Value::Object(items) => items.is_empty(),
        _ => false,
    }
}

fn collect_unique_ids<'a>(
    issues: &mut Vec<String>,
    items: impl IntoIterator<Item = (&'static str, &'a str)>,
) -> std::collections::BTreeSet<String> {
    let mut seen = std::collections::BTreeSet::new();
    for (kind, id) in items {
        if !seen.insert(id.to_string()) {
            issues.push(format!("duplicate {} id '{}'", kind, id));
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_spec_manifest() {
        let m =
            SpecManifest::new("ticket-api/store", "TicketStore", "ticket-api");
        assert_eq!(m.slug(), Some("ticket-api/store"));
        assert_eq!(m.title(), Some("TicketStore"));
        assert_eq!(m.component(), Some("ticket-api"));
        assert_eq!(m.state(), Some("draft"));
    }

    #[test]
    fn test_serde_round_trip() {
        let m =
            SpecManifest::new("ticket-api/store", "TicketStore", "ticket-api");
        let toml_str = toml::to_string_pretty(&m).unwrap();
        let m2: SpecManifest = toml::from_str(&toml_str).unwrap();
        assert_eq!(m2.slug(), Some("ticket-api/store"));
        assert_eq!(m2.title(), Some("TicketStore"));
        assert_eq!(m2.id(), m.id());
    }

    #[test]
    fn test_set_parent() {
        let mut m = SpecManifest::new(
            "ticket-api/store/create",
            "create",
            "ticket-api",
        );
        let parent_id = uuid::Uuid::new_v4().to_string();
        m.set_parent(&parent_id);
        assert_eq!(m.parent(), Some(parent_id.as_str()));
    }

    #[test]
    fn test_set_scope() {
        let mut m =
            SpecManifest::new("ticket-api/store", "TicketStore", "ticket-api");
        m.set_scope("public");
        assert_eq!(m.scope(), Some("public"));
    }

    #[test]
    fn test_contract_fields_round_trip_through_toml() {
        let mut manifest =
            SpecManifest::new("spec-api/contract", "Contract", "spec-api");
        manifest.set_contract_mode(Some(SpecContractMode::ExpectationOriented));
        manifest.set_expected_properties(vec![ExpectedProperty {
            id: "prop-visible".to_string(),
            statement: "Visible behavior is explicit.".to_string(),
        }]);
        manifest.set_acceptance_criteria(vec![AcceptanceCriterion {
            id: "criterion-visible".to_string(),
            statement: "The property is observable in store output."
                .to_string(),
            expected_property_ids: vec!["prop-visible".to_string()],
            required_evidence_ids: vec!["evidence-doc".to_string()],
        }]);
        manifest.set_evidence_requirements(vec![EvidenceRequirement {
            id: "evidence-doc".to_string(),
            kind: "documentation".to_string(),
            description: "A generated guidance check exists.".to_string(),
            optional: false,
        }]);
        manifest.set_fulfillment_summaries(vec![FulfillmentSummary {
            id: "summary-doc".to_string(),
            subject_kind: FulfillmentSubjectKind::EvidenceRequirement,
            subject_id: "evidence-doc".to_string(),
            status: FulfillmentStatus::Satisfied,
            detail: Some("Rule target check passed.".to_string()),
        }]);

        let toml_str = toml::to_string_pretty(&manifest).unwrap();
        let reparsed: SpecManifest = toml::from_str(&toml_str).unwrap();

        assert_eq!(
            reparsed.contract_mode(),
            Some(SpecContractMode::ExpectationOriented)
        );
        assert_eq!(reparsed.expected_properties().len(), 1);
        assert_eq!(reparsed.acceptance_criteria().len(), 1);
        assert_eq!(reparsed.evidence_requirements().len(), 1);
        assert_eq!(reparsed.fulfillment_summaries().len(), 1);
    }

    #[test]
    fn test_health_issues_ignore_legacy_specs_without_structured_contract() {
        let manifest =
            SpecManifest::new("spec-api/legacy", "Legacy", "spec-api");

        assert!(manifest.health_issues().is_empty());
    }

    #[test]
    fn test_health_issues_surface_missing_and_unsatisfied_contract_requirements()
     {
        let mut manifest = SpecManifest::new(
            "spec-api/contract-health",
            "Contract Health",
            "spec-api",
        );
        manifest.set_contract_mode(Some(SpecContractMode::ExpectationOriented));
        manifest.set_expected_properties(vec![ExpectedProperty {
            id: "prop-visible".to_string(),
            statement: "Visible behavior is explicit.".to_string(),
        }]);
        manifest.set_acceptance_criteria(vec![AcceptanceCriterion {
            id: "criterion-visible".to_string(),
            statement: "The property is observable in store output."
                .to_string(),
            expected_property_ids: vec!["prop-visible".to_string()],
            required_evidence_ids: vec!["evidence-doc".to_string()],
        }]);
        manifest.set_evidence_requirements(vec![EvidenceRequirement {
            id: "evidence-doc".to_string(),
            kind: "documentation".to_string(),
            description: "A generated guidance check exists.".to_string(),
            optional: false,
        }]);

        let issues = manifest.health_issues();
        assert!(issues.contains(
            &"missing fulfillment summary for evidence requirement 'evidence-doc'".to_string()
        ));

        manifest.set_fulfillment_summaries(vec![FulfillmentSummary {
            id: "summary-doc".to_string(),
            subject_kind: FulfillmentSubjectKind::EvidenceRequirement,
            subject_id: "evidence-doc".to_string(),
            status: FulfillmentStatus::Blocked,
            detail: Some("Validation is still blocked.".to_string()),
        }]);
        let issues = manifest.health_issues();
        assert!(issues.contains(
            &"unsatisfied evidence requirement 'evidence-doc'".to_string()
        ));

        manifest.set_fulfillment_summaries(vec![FulfillmentSummary {
            id: "summary-doc".to_string(),
            subject_kind: FulfillmentSubjectKind::EvidenceRequirement,
            subject_id: "evidence-doc".to_string(),
            status: FulfillmentStatus::Satisfied,
            detail: Some("Validation passed.".to_string()),
        }]);

        assert!(manifest.health_issues().is_empty());
    }
}
