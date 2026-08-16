//! Integration coverage for the ticket state-transition recovery contract.
//!
//! A blocked transition must explain how to recover: the current state, the
//! legally reachable next states, and the mandatory intermediate waypoints.
//! The `transitions` inspection command exposes the same legal transition graph.

mod common;

use common::{
    TicketCommands,
    TicketSandbox as Sandbox,
    create_ticket,
};

#[test]
fn blocked_transition_reports_current_allowed_and_intermediate_states() {
    let s = Sandbox::new();
    let id = create_ticket(&s, "Recovery contract ticket");

    // `open -> in-implementation` is reachable only through `planned`. Under the
    // `--single-hop` opt-out a direct jump must be rejected with recovery
    // guidance rather than auto-walked.
    let (code, stderr) = s.ticket_fail(&[
        "update",
        &id,
        "--to-state",
        "in-implementation",
        "--single-hop",
    ]);
    assert_ne!(code, 0, "blocked transition must exit non-zero");

    // Current state is named.
    assert!(
        stderr.contains("'open'"),
        "error should name the current state: {stderr}"
    );
    // Allowed single-hop next states are listed.
    assert!(
        stderr.contains("allows next states"),
        "error should list allowed next states: {stderr}"
    );
    assert!(
        stderr.contains("planned"),
        "error should list 'planned' as a legal next step: {stderr}"
    );
    assert!(
        stderr.contains("cancelled"),
        "error should list 'cancelled' as a legal next step: {stderr}"
    );
    // Mandatory intermediate waypoint to reach the requested target is named.
    assert!(
        stderr.contains("first transition through")
            && stderr.contains("planned"),
        "error should name the required intermediate state: {stderr}"
    );

    // The ticket must not have advanced.
    let got = s.ticket_json(&["get", &id]);
    assert_eq!(got["ticket"]["fields"]["state"], "open");
}

#[test]
fn transitions_command_shows_legal_transition_graph() {
    let s = Sandbox::new();
    let id = create_ticket(&s, "Transition inspection ticket");

    let overview = s.ticket_json(&["transitions", &id]);
    assert_eq!(overview["status"], "ok");
    assert_eq!(overview["current_state"], "open");

    let allowed: Vec<&str> = overview["allowed_next_states"]
        .as_array()
        .expect("allowed_next_states array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        allowed.contains(&"planned"),
        "allowed next from new should include ready: {allowed:?}"
    );
    assert!(
        allowed.contains(&"cancelled"),
        "allowed next from new should include cancelled: {allowed:?}"
    );

    // The full transition graph and declared states are present.
    assert!(
        overview["transitions"]
            .as_array()
            .is_some_and(|t| !t.is_empty()),
        "transition graph should be non-empty"
    );
    let states: Vec<&str> = overview["states"]
        .as_array()
        .expect("states array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(states.contains(&"in-implementation"));
    assert!(states.contains(&"done"));
}

#[test]
fn legal_single_hop_transition_still_succeeds() {
    let s = Sandbox::new();
    let id = create_ticket(&s, "Legal hop ticket");

    // A direct, legal single-hop transition remains unaffected.
    let updated = s.ticket_json(&["update", &id, "--to-state", "planned"]);
    assert_eq!(updated["status"], "ok");
    let got = s.ticket_json(&["get", &id]);
    assert_eq!(got["ticket"]["fields"]["state"], "planned");
}

#[test]
fn multi_hop_transition_auto_walks_by_default() {
    let s = Sandbox::new();
    let id = create_ticket(&s, "Auto-walk ticket");

    // `open -> in-implementation` requires passing through `planned`. Without the
    // `--single-hop` opt-out the update auto-walks the path and succeeds.
    let updated =
        s.ticket_json(&["update", &id, "--to-state", "in-implementation"]);
    assert_eq!(updated["status"], "ok");
    let got = s.ticket_json(&["get", &id]);
    assert_eq!(got["ticket"]["fields"]["state"], "in-implementation");
}
