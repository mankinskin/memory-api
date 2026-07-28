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

use crate::{
    code_ref::CodeRef,
    ticket_ref::TicketRef,
};

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

    /// Structured ticket links for this spec (typed field backed by the
    /// `related_tickets` extra key). Returns an empty vec (never errors)
    /// when the key is absent or holds legacy untyped entries — see
    /// [`Self::legacy_ticket_link_entries`] for the migration-detection
    /// path.
    pub fn related_tickets(&self) -> Vec<TicketRef> {
        self.parse_vec_field("related_tickets")
    }

    /// Legacy untyped ticket-link entries (bare UUID or path strings) found
    /// under the `related_tickets`/`ticket_ids` extra keys. Used by
    /// `validate-links` and migration tooling to detect specs that still
    /// need conversion to structured [`TicketRef`] entries; never an error
    /// on its own.
    pub fn legacy_ticket_link_entries(&self) -> Vec<String> {
        let mut entries = Vec::new();
        for key in ["related_tickets", "ticket_ids"] {
            if let Some(Value::Array(items)) = self.extra.get(key) {
                for item in items {
                    if let Value::String(s) = item {
                        entries.push(s.clone());
                    }
                }
            }
        }
        entries
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

    /// Replace the structured ticket links, storing them under the
    /// `related_tickets` extra key. Removes the key entirely when empty
    /// (via `set_typed_field`'s empty-vec handling) so serialized manifests
    /// stay minimal, and so a migrated manifest no longer carries the
    /// legacy untyped entries once replaced.
    pub fn set_related_tickets(
        &mut self,
        related_tickets: Vec<TicketRef>,
    ) {
        self.set_typed_field("related_tickets", related_tickets);
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

        let parsed = self.parse_structured_contract_fields(&mut issues);
        let expected_properties = parsed.expected_properties;
        let acceptance_criteria = parsed.acceptance_criteria;
        let evidence_requirements = parsed.evidence_requirements;
        let fulfillment_summaries = parsed.fulfillment_summaries;

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

        validate_acceptance_criterion_links(
            &mut issues,
            &acceptance_criteria,
            &expected_property_ids,
            &evidence_requirement_ids,
        );
        validate_fulfillment_summary_targets(
            &mut issues,
            &fulfillment_summaries,
            &acceptance_criterion_ids,
            &evidence_requirement_ids,
        );
        validate_required_evidence_fulfillment(
            &mut issues,
            &evidence_requirements,
            &fulfillment_summaries,
        );

        issues
    }

    fn parse_structured_contract_fields(
        &self,
        issues: &mut Vec<String>,
    ) -> ParsedStructuredContractFields {
        ParsedStructuredContractFields {
            expected_properties: parse_structured_field(
                self,
                issues,
                "expected_properties",
                "invalid expected properties",
            ),
            acceptance_criteria: parse_structured_field(
                self,
                issues,
                "acceptance_criteria",
                "invalid acceptance criteria",
            ),
            evidence_requirements: parse_structured_field(
                self,
                issues,
                "evidence_requirements",
                "invalid evidence requirements",
            ),
            fulfillment_summaries: parse_structured_field(
                self,
                issues,
                "fulfillment_summaries",
                "invalid fulfillment summaries",
            ),
        }
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

struct ParsedStructuredContractFields {
    expected_properties: Vec<ExpectedProperty>,
    acceptance_criteria: Vec<AcceptanceCriterion>,
    evidence_requirements: Vec<EvidenceRequirement>,
    fulfillment_summaries: Vec<FulfillmentSummary>,
}

fn parse_structured_field<T>(
    manifest: &SpecManifest,
    issues: &mut Vec<String>,
    key: &str,
    error_prefix: &str,
) -> Vec<T>
where
    T: DeserializeOwned,
{
    match manifest.parse_field::<Vec<T>>(key) {
        Ok(Some(values)) => values,
        Ok(None) => Vec::new(),
        Err(error) => {
            issues.push(format!("{error_prefix}: {error}"));
            Vec::new()
        },
    }
}

fn validate_acceptance_criterion_links(
    issues: &mut Vec<String>,
    acceptance_criteria: &[AcceptanceCriterion],
    expected_property_ids: &std::collections::BTreeSet<String>,
    evidence_requirement_ids: &std::collections::BTreeSet<String>,
) {
    for criterion in acceptance_criteria {
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
}

fn validate_fulfillment_summary_targets(
    issues: &mut Vec<String>,
    fulfillment_summaries: &[FulfillmentSummary],
    acceptance_criterion_ids: &std::collections::BTreeSet<String>,
    evidence_requirement_ids: &std::collections::BTreeSet<String>,
) {
    for summary in fulfillment_summaries {
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
}

fn validate_required_evidence_fulfillment(
    issues: &mut Vec<String>,
    evidence_requirements: &[EvidenceRequirement],
    fulfillment_summaries: &[FulfillmentSummary],
) {
    for evidence in evidence_requirements {
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
#[path = "manifest/tests.rs"]
mod tests;
