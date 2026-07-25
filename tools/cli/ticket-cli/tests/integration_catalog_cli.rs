//! Integration coverage for the CLI `catalog` capability surface.
//!
//! Asserts the human/machine-readable catalog lists the ticket/spec/rule
//! workflows, a rule-oriented workflow, and nested-root support — matching the
//! ticket-MCP `ticket_capabilities` tool (both read the same shared catalog).

mod common;

use common::{
    TicketCommands,
    TicketSandbox as Sandbox,
};

#[test]
fn catalog_lists_ticket_spec_rule_workflows() {
    let s = Sandbox::new();
    let catalog = s.ticket_json(&["catalog"]);

    assert_eq!(catalog["status"], "ok");

    let domains: Vec<&str> = catalog["domains"]
        .as_array()
        .expect("domains array")
        .iter()
        .filter_map(|d| d["domain"].as_str())
        .collect();
    assert!(domains.contains(&"ticket"));
    assert!(domains.contains(&"spec"));
    assert!(domains.contains(&"rule"));

    // Rule-oriented workflow discoverable from the catalog.
    let rule = catalog["domains"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["domain"] == "rule")
        .expect("rule domain");
    assert!(
        rule["workflows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["name"].as_str().is_some_and(|n| n.contains("generate"))),
        "rule domain must expose an authoring/generation workflow"
    );

    // Nested-root support is declared per workflow.
    for domain in catalog["domains"].as_array().unwrap() {
        for workflow in domain["workflows"].as_array().unwrap() {
            assert!(
                workflow["nested_roots_supported"].is_boolean(),
                "every workflow must declare nested-root support"
            );
        }
    }

    // Parity gaps are explicit.
    assert!(catalog["parity_gaps"].as_array().is_some_and(|g| !g.is_empty()));
}
