mod common;

use common::{Sandbox, create_ticket};

#[test]
fn ready_overview_returns_json_with_ready_tickets() {
    let s = Sandbox::new();

    let blocked = create_ticket(&s, "Blocked ticket");
    let done_dependency = create_ticket(&s, "Done dependency");
    let ready = create_ticket(&s, "Ready ticket");

    s.ticket_json(&["update", &done_dependency, "--to-state", "in-refinement"]);
    s.ticket_json(&["update", &done_dependency, "--to-state", "ready"]);
    s.ticket_json(&["update", &done_dependency, "--to-state", "in-implementation"]);
    s.ticket_json(&["update", &done_dependency, "--to-state", "in-review"]);
    s.ticket_json(&["update", &done_dependency, "--to-state", "in-validation"]);
    s.ticket_json(&["update", &done_dependency, "--to-state", "done"]);

    s.ticket_json(&[
        "link",
        "--from",
        &blocked,
        "--to",
        &ready,
        "--kind",
        "depends_on",
    ]);

    let result = s.ticket_json(&[
        "ready-overview",
        "--scope",
        "integration test scope",
    ]);

    assert_eq!(result["status"], "ok");
    assert_eq!(result["ready_count"], 1);
    assert_eq!(result["scope"], "integration test scope");
    assert_eq!(result["summary"]["ready"], 1);

    let ready_items = result["ready"].as_array().expect("ready should be an array");
    assert_eq!(ready_items.len(), 1);
    assert_eq!(ready_items[0]["id"], ready);
    assert_eq!(ready_items[0]["title"], "Ready ticket");
}
