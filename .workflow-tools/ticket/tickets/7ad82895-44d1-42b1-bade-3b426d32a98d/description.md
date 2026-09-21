## Symptom

`cargo test -p ticket-http` fails 5 of 78 tests (verified independently, both parallel and single-threaded — same 5 failures either way, ruling out test-isolation/ordering):

1. `serve::registry::workspace_resolution_tests::manifest_only_hidden_child_store_is_discovered` (workspace_resolution_tests.rs:192)
2. `serve::registry::workspace_resolution_tests::resolve_indexed_many_prefers_deepest_existing_workspace` (workspace_resolution_tests.rs:229)
3. `serve::routes::tests::ancestor_graph_ref_from_child_workspace_is_followable` (routes/tests.rs:439)
4. `serve::routes::tests::descendant_ticket_ref_from_list_is_followable` (routes/tests.rs:365)
5. `serve::routes::tests::workspace_graph_includes_isolated_local_and_cross_workspace_nodes` (routes/tests.rs:541)

Verbatim panics (single-threaded, `--test-threads=1`, from `target/debug/deps/ticket_http-*.exe`):

```
---- serve::registry::workspace_resolution_tests::manifest_only_hidden_child_store_is_discovered stdout ----
thread '...' panicked at .../registry/workspace_resolution_tests.rs:220:5:
assertion failed: registry.workspace_infos().into_iter().any(|info| info.label == "memory-api")

---- serve::registry::workspace_resolution_tests::resolve_indexed_many_prefers_deepest_existing_workspace stdout ----
thread '...' panicked at .../registry/workspace_resolution_tests.rs:279:5:
assertion `left == right` failed
  left: "child--5cd6b1cb"
 right: "child"

---- serve::routes::tests::ancestor_graph_ref_from_child_workspace_is_followable stdout ----
thread '...' panicked at .../routes/tests.rs:477:10:
add mixed-workspace edge: NotFound(4e654d49-8abc-4da4-8acc-a16bfc885794)

---- serve::routes::tests::descendant_ticket_ref_from_list_is_followable stdout ----
thread '...' panicked at .../routes/tests.rs:410:5:
assertion `left == right` failed
  left: String("child--140b6abd")
 right: "child"

---- serve::routes::tests::workspace_graph_includes_isolated_local_and_cross_workspace_nodes stdout ----
thread '...' panicked at .../routes/tests.rs:590:10:
add mixed-workspace edge: NotFound(ef925b22-3506-4bab-af5c-a6c3320fc286)
```

## Root cause

Commit `3471427` ("Introduce collision-safe workspace identity", 2026-05-21) changed `canonical_workspace_name_for_index_root()` in `memory-api/tools/http/ticket-http/src/serve/registry.rs` to append a short content hash to the workspace *name* used as the registry key and as the `workspace` field on `ResolvedIndexedTicket` / ticket-ref JSON payloads (e.g. `"child--5cd6b1cb"` instead of plain `"child"`). The plain, human-readable string is still available separately as `label` (`WorkspaceNameInfo.label`).

Two of the five failing tests (`resolve_indexed_many_prefers_deepest_existing_workspace`, `descendant_ticket_ref_from_list_is_followable`) were written on 2026-05-20 (commit `cf49d4c`), *before* the hash-suffix behavior existed, and assert the bare label (`"child"`) where the code now returns the canonical hash-suffixed name. They were never updated when `3471427` landed the day after.

The other two routes tests (`ancestor_graph_ref_from_child_workspace_is_followable`, `workspace_graph_includes_isolated_local_and_cross_workspace_nodes`) fail with `NotFound(<uuid>)` when adding a "mixed-workspace edge" — almost certainly the same root cause: an edge-add call is passed a workspace/ticket ref built from the bare label, which no longer resolves to a registered workspace key once the hash suffix became mandatory.

`manifest_only_hidden_child_store_is_discovered` is related but may be a second, distinct gap: the test only creates an empty `tickets/` directory (`std::fs::create_dir_all(child_index_root.join("tickets"))`) without ever writing a ticket manifest file into it, then asserts the child workspace is discovered as "manifest-only". `has_ticket_manifest()` in registry.rs requires at least one manifest entry under `tickets/`, so an empty directory can never be discovered this way — either the test fixture is missing a manifest-file write, or `detect_store_root`/`has_ticket_manifest` needs to treat a bare `tickets/` directory as sufficient. Needs a judgment call on intended contract.

## Why not fixed directly

This is a genuine API/contract question, not a one-line fixture fix:
- Is the hash-suffixed canonical name the intended value for the public `workspace` field in ticket-ref JSON and `ResolvedIndexedTicket.workspace` (in which case the 4 identity-related tests need their expectations updated to match, and the edge-add NotFound tests need their edge-target construction updated to use resolved/canonical names)? The collision-safe identity feature was deliberately introduced to avoid ambiguity between same-named workspaces at different paths, so keeping the hash suffix looks intentional.
- Or should call sites that build `?workspace=` query params / edge targets resolve through `label`/legacy-alias lookup transparently, and something in that resolution path regressed?

Either direction touches `resolve_workspace_name`/`resolve_indexed_many`/edge-add code paths in `memory-api/tools/http/ticket-http/src/serve/registry.rs` and `routes.rs`, which affects the public HTTP contract — out of scope for a same-day triage pass, and the assigned scope for this triage explicitly excludes risky product-behavior changes.

## Suggested fix direction

1. Decide (with a spec update) whether ticket-ref/JSON `workspace` values are the canonical hash-suffixed name or the plain label going forward.
2. If canonical name is correct: update the 4 identity/edge tests' expectations and edge-target construction to use `registry.resolve_workspace_name(...)` / the canonical name instead of a hard-coded `"child"` literal.
3. If label is correct: make `resolve_indexed_many` and the edge-add path resolve/emit the label (or resolve labels transparently at the API boundary) instead of the raw registry key.
4. Separately fix `manifest_only_hidden_child_store_is_discovered`: either write an actual ticket file/manifest under `tickets/` in the fixture, or extend `has_ticket_manifest`/`detect_store_root` to recognize a bare `tickets/` directory as a discoverable (but empty) manifest-only store — confirm which is intended before changing.

## Verification performed

- Reproduced with `cargo test -p ticket-http` (parallel, default) and again with the compiled test binary run directly `--test-threads=1` (fully serialized) — identical 5 failures both times, ruling out cross-test interference/shared temp-dir races.
- Confirmed via `git log`/`git show` that the label-vs-canonical-name test expectations predate the hash-suffix feature by one day (`cf49d4c` 2026-05-20 vs `3471427` 2026-05-21), and that neither commit nor any later one reconciled them — these are stale since 2026-05-21, unrelated to the current frozen-part-gate/`description_mode` work (which lands 2026-07-30 and only touches `ticket-api` storage/update code, not `ticket-http` registry/routes).
- Also fixed 2 sibling failures in the same original 7-test batch that were a different, clearly-correct root cause (missing `TicketStore::init()` in bare-tempdir test fixtures, from commit `3103297`'s incomplete "all test callsites updated" claim) — see `serve::registry::tests::concurrent_get_returns_shared_store_instance` and `serve::tests::ensure_workspace_runtime_wires_hook_for_lazy_open_store`, now passing.

## Scope note

Confine any follow-up fix to `memory-api/tools/http/ticket-http/src/serve/**` per the parallel-work constraints active on 2026-07-30 (another agent owns `ticket-api`, another owns tool `Cargo.toml`/`tests/` trybuild fixtures).
