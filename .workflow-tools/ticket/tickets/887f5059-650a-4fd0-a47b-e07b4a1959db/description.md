## Symptom

- `serve::routes::tests::ancestor_graph_ref_from_child_workspace_is_followable` ([memory-api/crates/ticket/src/serve/routes/tests.rs](memory-api/crates/ticket/src/serve/routes/tests.rs#L448))
- `serve::routes::tests::workspace_graph_includes_isolated_local_and_cross_workspace_nodes` ([memory-api/crates/ticket/src/serve/routes/tests.rs](memory-api/crates/ticket/src/serve/routes/tests.rs#L550))

Both tests fail during fixture setup, before either HTTP graph route runs. The shared `cargo test -p ticket --all-features` result was:

```text
test result: FAILED. 128 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 16.44s
```

Each test panics at the mixed-workspace edge-add step with `NotFound`:

```text
add mixed-workspace edge: NotFound(c16f0b57-17b9-48f5-9570-9a5a57f4b641)
add mixed-workspace edge: NotFound(3876562b-b5c3-4de0-9243-1b21e1635555)
```

The relevant fixture setup lines are:

```rust
let parent_store = open_workspace_store(dir.path());
let child_store = open_workspace_store(&child_dir);

let parent_id = parent_store
    .create(
        None,
        "tracker-improvement",
        Some("Parent ticket"),
        None,
        BTreeMap::new(),
        None,
        None,
    )
    .expect("create parent ticket");
let child_id = child_store
    .create(
        None,
        "tracker-improvement",
        Some("Child ticket"),
        None,
        BTreeMap::new(),
        None,
        None,
    )
    .expect("create child ticket");

child_store
    .add_edge(ticket_api::model::edge::EdgeRecord {
        from: child_id,
        to: parent_id,
        kind: "depends_on".into(),
        created_at: chrono::Utc::now(),
    })
    .expect("add mixed-workspace edge");
```

## Provenance

- `git log` attributes the crate move to `68026dcf`, the extraction of the ticket tool into `memory-api/crates/ticket`.
- `git blame` attributes the original test definitions to `d2852972e`, when the tests lived under the legacy `ticket-http` layout.
- None of the commits on this branch (`717c3329`, `7ad02b3b`, `986d6c1a`, `30375247`, `547548da`, `77d32466`) touch ticket crate source.

## Open question

The diagnosis calls this a stale test assumption, but that is not settled. Two hypotheses remain:

- **(A) Stale test.** The tests encode a cross-workspace edge-creation capability that the store intentionally no longer permits after the extraction, so the fixture is invalid and the tests should be rewritten or removed.
- **(B) Product regression.** Cross-workspace edge creation is still a supported capability, and the extraction broke the child store's ability to resolve a parent-store UUID, so the tests are correct and the store is wrong.

The ticket should record that deciding between those hypotheses requires knowing whether mixed-workspace edges are an intended product capability.

## Acceptance criteria

- `cargo test -p ticket --all-features` is green.
- The resolution explicitly states whether mixed-workspace edges are a supported capability, so the fix cannot be a silent test deletion.
