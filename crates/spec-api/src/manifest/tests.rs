use super::*;

#[test]
fn test_new_spec_manifest() {
    let m = SpecManifest::new("ticket-api/store", "TicketStore", "ticket-api");
    assert_eq!(m.slug(), Some("ticket-api/store"));
    assert_eq!(m.title(), Some("TicketStore"));
    assert_eq!(m.component(), Some("ticket-api"));
    assert_eq!(m.state(), Some("draft"));
}

#[test]
fn test_serde_round_trip() {
    let m = SpecManifest::new("ticket-api/store", "TicketStore", "ticket-api");
    let toml_str = toml::to_string_pretty(&m).unwrap();
    let m2: SpecManifest = toml::from_str(&toml_str).unwrap();
    assert_eq!(m2.slug(), Some("ticket-api/store"));
    assert_eq!(m2.title(), Some("TicketStore"));
    assert_eq!(m2.id(), m.id());
}

#[test]
fn test_set_parent() {
    let mut m =
        SpecManifest::new("ticket-api/store/create", "create", "ticket-api");
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
        statement: "The property is observable in store output.".to_string(),
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
    let manifest = SpecManifest::new("spec-api/legacy", "Legacy", "spec-api");

    assert!(manifest.health_issues().is_empty());
}

#[test]
fn test_health_issues_surface_missing_and_unsatisfied_contract_requirements() {
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
        statement: "The property is observable in store output.".to_string(),
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
        &"missing fulfillment summary for evidence requirement 'evidence-doc'"
            .to_string(),
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
        &"unsatisfied evidence requirement 'evidence-doc'".to_string(),
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
