//! Self-describing capability catalog for the ticket / spec / rule surfaces.
//!
//! This is the single canonical description of the common operator/agent
//! workflows across the three entity domains. It is intentionally hosted in
//! `ticket-api` (a shared dependency) so that every consumer — the ticket CLI
//! `catalog` command and the ticket-MCP `ticket_capabilities` tool — renders
//! the *same* data and therefore agrees on the named workflows, required
//! parameters, and nested-root/store targeting semantics by construction.
//!
//! The catalog covers, per domain:
//! - read flows
//! - mutation flows
//! - board / next / why-not flows (ticket)
//! - validation flows
//! - at least one rule-oriented workflow
//!
//! It also records the known cross-surface parity gaps so agents can pick the
//! correct fallback immediately instead of discovering a missing surface by
//! trial and error.

use serde_json::{
    Value,
    json,
};

/// Build the machine-readable ticket/spec/rule capability catalog.
///
/// Consumers should present this verbatim for machine clients (MCP/JSON/TOON)
/// and may render a condensed human-readable form from the same structure.
pub fn capability_catalog() -> Value {
    json!({
        "catalog": "ticket-spec-rule-capabilities",
        "version": 1,
        "summary": "Canonical ticket/spec/rule workflows with required params and \
                    nested-root targeting. One surface so CLI and MCP agree.",
        "domains": [
            ticket_domain(),
            spec_domain(),
            rule_domain(),
        ],
        "parity_gaps": parity_gaps(),
    })
}

fn ticket_domain() -> Value {
    json!({
        "domain": "ticket",
        "cli": "ticket",
        "mcp_prefix": "mcp_ticket-mcp_",
        "nested_roots_supported": true,
        "nested_roots_note": "Reads against the `default` workspace aggregate \
             all descendant `.ticket` stores. Writes (`create`/`update`) target \
             the store that owns the `workspace` you pass; pass a descendant \
             store path to co-locate new tickets there.",
        "workflows": [
            {
                "name": "read",
                "purpose": "Fetch or enumerate tickets and descriptions.",
                "cli": ["get <id>", "describe <id>", "list [--where k=v]", "search <query>"],
                "mcp": ["get_ticket", "get_ticket_description", "list_tickets", "subgraph", "topgraph"],
                "required_params": {"workspace": "store selector (default or path)"},
                "nested_roots_supported": true,
            },
            {
                "name": "mutate",
                "purpose": "Create tickets and apply field/state changes.",
                "cli": ["create --type <t> --title <s> --workspace <path>", "update <id> --to-state <s>", "close <id>", "cancel <id>"],
                "mcp": ["create_ticket", "update_ticket", "close_ticket", "cancel_ticket"],
                "required_params": {"workspace": "target store (explicit for create)"},
                "nested_roots_supported": true,
                "note": "`update --to-state` auto-walks the shortest legal \
                         path by default, visiting required waypoints; pass \
                         `--single-hop` (CLI) / `single_hop` (MCP) to opt into \
                         strict one-hop mode that rejects skipped-waypoint \
                         transitions with recovery guidance; `close` \
                         fast-forwards the shortest legal path.",
            },
            {
                "name": "transitions-inspection",
                "purpose": "Show the legal state-transition graph for a ticket.",
                "cli": ["transitions <id>"],
                "mcp": [],
                "required_params": {"id": "ticket id/prefix"},
                "nested_roots_supported": true,
                "note": "Returns current state, allowed next states, full \
                         transition graph, required and terminal states. Pairs \
                         with the invalid-transition recovery error shape.",
            },
            {
                "name": "board",
                "purpose": "Coordinate work-in-progress ownership and leases.",
                "cli": ["board show", "board check-in <id> --agent <a>", "board check-out <id>", "board heartbeat <entry>"],
                "mcp": ["board_show", "board_check_in", "board_check_out", "board_heartbeat", "board_clean_preview", "board_clean_apply"],
                "required_params": {"workspace": "store selector"},
                "nested_roots_supported": true,
            },
            {
                "name": "next-and-why-not",
                "purpose": "Pick actionable work and explain what blocks a ticket.",
                "cli": ["next [--root <id>]", "blockers <id>", "unblocked-by <id>", "ready-overview"],
                "mcp": ["next_tickets", "subgraph", "topgraph"],
                "required_params": {"workspace": "store selector"},
                "nested_roots_supported": true,
            },
            {
                "name": "validation",
                "purpose": "Record and query validation evidence linked to tickets.",
                "cli": ["(test CLI) test record-spec", "test record-execution", "test list-executions --ticket-id <id>"],
                "mcp": ["mcp_test-mcp_test_record_spec", "mcp_test-mcp_test_record_execution", "mcp_test-mcp_test_list_executions"],
                "required_params": {"ticket_ids": "link evidence back to tickets"},
                "nested_roots_supported": true,
                "note": "Validation evidence is owned by test-api, not ticket-mcp. \
                         Link via `ticket_ids` instead of inlining results.",
            },
        ],
    })
}

fn spec_domain() -> Value {
    json!({
        "domain": "spec",
        "cli": "spec",
        "mcp_prefix": "mcp_spec-mcp_",
        "nested_roots_supported": true,
        "nested_roots_note": "Spec tools aggregate every discoverable `.spec` \
             store unless `workspace` pins to one. Writes target the addressed store.",
        "workflows": [
            {
                "name": "read",
                "purpose": "Fetch or enumerate specs and their hierarchy.",
                "cli": ["spec get <id> [--full]", "spec list [--where k=v]", "spec search <query>", "spec tree [<id>]"],
                "mcp": ["spec_get", "spec_list", "spec_search", "spec_tree", "spec_section_get", "spec_section_list"],
                "required_params": {"workspace": "store selector (optional)"},
                "nested_roots_supported": true,
            },
            {
                "name": "mutate",
                "purpose": "Create specs and edit fields, state, sections, body.",
                "cli": ["spec create --title <s> --slug <s> --component <c> --workspace <path>", "spec update <id>", "spec section add <id> <name>"],
                "mcp": ["spec_create", "spec_update", "spec_section_add", "spec_section_delete"],
                "required_params": {"workspace": "target store"},
                "nested_roots_supported": true,
            },
            {
                "name": "validation",
                "purpose": "Check spec health and code-reference integrity.",
                "cli": ["spec health --all", "spec refs validate <id>"],
                "mcp": ["spec_health", "spec_refs_validate"],
                "required_params": {"id": "spec id/slug"},
                "nested_roots_supported": true,
            },
        ],
    })
}

fn rule_domain() -> Value {
    json!({
        "domain": "rule",
        "cli": "rule",
        "mcp_prefix": "mcp_rule-mcp_",
        "nested_roots_supported": true,
        "nested_roots_note": "Rule scan roots are registered explicitly \
             (`rule add-root`); scans reindex all registered roots.",
        "workflows": [
            {
                "name": "read",
                "purpose": "Search and inspect rule entries.",
                "cli": ["rule search <query>", "rule list [--section <s>]", "rule get <id>"],
                "mcp": ["rule_search", "rule_list", "rule_get"],
                "required_params": {},
                "nested_roots_supported": true,
            },
            {
                "name": "mutate",
                "purpose": "Create and update rule entries and feedback.",
                "cli": ["rule create --title <s> --slug <s> --file-kind <k> --section <s> --workspace <path>", "rule update <id>", "rule record-feedback <id> --rating <r>"],
                "mcp": ["rule_create", "rule_update", "rule_record_feedback"],
                "required_params": {"workspace": "target store"},
                "nested_roots_supported": true,
            },
            {
                "name": "author-and-generate",
                "purpose": "Rule-oriented flow: register roots, scan, then render \
                            deterministic instruction files from canonical rules.",
                "cli": ["rule add-root <path>", "rule scan", "rule import-file <path>", "rule generate-file --file-kind <k> --repo-scope <r>", "rule generate-target --config <p> --target <t>"],
                "mcp": ["rule_add_root", "rule_scan", "rule_import_file", "rule_generate_file", "rule_generate_target", "rule_explain_target"],
                "required_params": {"file_kind": "e.g. agents-md", "repo_scope": "target repo"},
                "nested_roots_supported": true,
                "note": "This is the canonical rule authoring workflow: import or \
                         create rules, then generate provenance-stamped markdown.",
            },
        ],
    })
}

fn parity_gaps() -> Value {
    json!([
        {
            "gap": "ticket validation evidence",
            "detail": "ticket-mcp does not store validation results; use test-api \
                       (test-mcp / test CLI) and link via `ticket_ids`.",
        },
        {
            "gap": "cross-domain MCP servers",
            "detail": "spec and rule workflows are served by separate MCP servers \
                       (spec-mcp, rule-mcp). This catalog names their canonical \
                       flows but is emitted from the ticket surface; call the \
                       corresponding server for execution.",
        },
        {
            "gap": "doc deep-links",
            "detail": "doc-viewer has no stable per-entity deep-link route yet; \
                       reference doc artifacts by package::target until one exists.",
        },
        {
            "gap": "transitions inspection over MCP",
            "detail": "The `transitions` inspection command is CLI-only today; \
                       ticket-mcp surfaces the same allowed-next/intermediate \
                       fields via the invalid-transition error on `update_ticket`.",
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_ticket_spec_and_rule_domains() {
        let catalog = capability_catalog();
        let domains: Vec<&str> = catalog["domains"]
            .as_array()
            .expect("domains array")
            .iter()
            .filter_map(|d| d["domain"].as_str())
            .collect();
        assert!(domains.contains(&"ticket"));
        assert!(domains.contains(&"spec"));
        assert!(domains.contains(&"rule"));
    }

    #[test]
    fn catalog_includes_a_rule_oriented_workflow() {
        let catalog = capability_catalog();
        let rule = catalog["domains"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["domain"] == "rule")
            .expect("rule domain");
        let names: Vec<&str> = rule["workflows"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|w| w["name"].as_str())
            .collect();
        assert!(
            names.contains(&"author-and-generate"),
            "catalog must list the canonical rule authoring workflow: {names:?}"
        );
    }

    #[test]
    fn every_workflow_declares_nested_root_support() {
        let catalog = capability_catalog();
        for domain in catalog["domains"].as_array().unwrap() {
            for workflow in domain["workflows"].as_array().unwrap() {
                assert!(
                    workflow["nested_roots_supported"].is_boolean(),
                    "workflow {:?} in domain {:?} must declare nested-root support",
                    workflow["name"],
                    domain["domain"],
                );
            }
        }
    }

    #[test]
    fn catalog_documents_parity_gaps() {
        let catalog = capability_catalog();
        assert!(
            catalog["parity_gaps"].as_array().is_some_and(|g| !g.is_empty()),
            "catalog must document parity gaps"
        );
    }
}
