# Problem

The session workflow `kind` field conflates two orthogonal concerns and gets the restriction
axis wrong:

- **Behavioral role** — whether a node gates finish/handoff. Only `Ticket` and `Validation`
  carry this weight today.
- **Descriptive category** — what sort of work a node represents. `Action`, `Decision`,
  `Checkpoint` are pure labels with no behavior.

Evidence:
- `Ticket` is load-bearing: a *required* `Ticket` node whose live state cannot resolve fails
  finish closed (`memory-api/crates/session-api/src/store/config/handoff_finish.rs` ~L199-211);
  `promote` sets it and requires `ticket_urn` (`.../store/config/workflow.rs` ~L101).
- `Validation` is load-bearing: *required* `Validation` nodes drive validation-gate resolution
  at finish and must carry `validation_spec_id` (`.../store/config/persistence.rs` ~L18-27).
- `Action`, `Decision`, `Checkpoint` match **nowhere** in production logic (grep: tests only;
  `Decision` matches nothing at all). `SessionWorkflowNode` already has a free-text `title`, so
  descriptive nuance already has a home.
- No spec documents the taxonomy.

Two consequences:
1. **Removing the restriction (free-string kind) is unsafe.** Because `Ticket`/`Validation` gate
   safety-critical finish logic, a typo like `"tickett"` would silently disable the gate —
   fail-open on exactly the checks meant to prevent incomplete handoffs.
2. **The one genuinely missing capability is a `Spec` behavioral kind** — symmetric to `Ticket`,
   carrying a `spec_urn`, so finish can gate on required spec nodes the way it gates on tickets.
   Sessions pin specs but cannot gate on spec state.

# Decision (design exploration)

Separate the two axes rather than loosen or blindly extend `kind`:

1. Keep a **closed, validated set of behavioral kinds** that code branches on and that carry
   required side-data: `Ticket`, `Validation`, and a **new `Spec`** kind.
2. Move descriptive classification to an **open free-text field** (e.g. `category: Option<String>`)
   and/or lean on the existing `title`, so agents never hit an expressiveness wall for labels that
   do not drive behavior.
3. Deprecate the cosmetic `Decision` (used nowhere) and reduce `Action`/`Checkpoint` to generic
   descriptive buckets or fold them into the descriptive field, with back-compat for persisted data.

# Scope

1. Add `SessionWorkflowNodeKind::Spec` in `memory-api/crates/session-api/src/model/workflow.rs`
   with a `spec_urn: Option<String>` field on the node (mirror of `ticket_urn`).
2. Extend finish/handoff gating so a *required* `Spec` node resolves spec state and fails closed
   when unavailable, mirroring the existing `Ticket` behavior; add a `promote`-to-spec path if
   symmetric to ticket promotion.
3. Introduce an open descriptive field on the node (free text, no code branches on it) for
   categorization; document that behavioral kinds stay closed while description is unbounded.
4. Deprecate `Decision`; decide the fate of `Action`/`Checkpoint` (keep as generic buckets vs
   migrate into the descriptive field). Preserve serde back-compat so existing persisted runtime
   contexts with `action`/`decision`/`checkpoint` still deserialize and are mapped sensibly.
5. Update `session-cli` and `session-mcp` parsers/help and the CLI/MCP arg docs to match the new
   behavioral-kind set (coordinate with `7f1ed44f`, which enum-constrains the MCP schema, and
   `8bb97b73`, which lists allowed values on rejection).
6. Author/refresh a spec documenting the node-kind taxonomy (behavioral vs descriptive axes).

# Coordination

- `7f1ed44f` (enum-constrain workflow mutation params) advertises the enum; this ticket changes
  what the enum *contains* and adds the descriptive field — must land coherently.
- `8bb97b73` (invalid-enum recovery contract) must reflect the new behavioral-kind set in its
  allowed-values message.

# Regression Validation Requirements

- **Unit:** required `Spec` node fails finish closed when spec state is unavailable; passes when done.
- **Unit:** descriptive/category field never affects finish gating.
- **Back-compat:** persisted contexts using `action`/`decision`/`checkpoint` deserialize without error and map to the descriptive field/generic bucket.
- **Schema:** MCP schema advertises the new behavioral-kind set (parity with `7f1ed44f`).
- **Docs/spec:** taxonomy spec exists and matches enforced behavior.

# Acceptance Criteria

- Behavioral kinds are a closed validated set: `Ticket`, `Validation`, `Spec`.
- A required `Spec` node gates finish symmetrically to `Ticket` and carries `spec_urn`.
- Node descriptive classification is available via an open free-text field that no code branches on.
- `Decision` is removed/deprecated; `Action`/`Checkpoint` fate is resolved with back-compat for persisted data.
- CLI/MCP help, schema, and the recovery-error allowed-values all reflect the new behavioral-kind set.
- A spec documents the behavioral-vs-descriptive taxonomy.

# Likely Surfaces

- `memory-api/crates/session-api/src/model/workflow.rs`
- `memory-api/crates/session-api/src/store/config/handoff_finish.rs`
- `memory-api/crates/session-api/src/store/config/persistence.rs`
- `memory-api/crates/session-api/src/store/config/workflow.rs`
- `memory-api/tools/mcp/session-mcp/src/server.rs`
- `memory-api/tools/cli/session-cli/src/lib.rs`
- `memory-api/.spec/`

# Implementation Status — in-review (2026-07-25)

Delivered: `SessionWorkflowNodeKind` is now the closed behavioral set `{Ticket, Validation, Spec}` plus the generic descriptive `Task`; added `spec_urn` and open free-text `category` on `SessionWorkflowNode`/`SessionWorkflowNodeDraft`. A required `Spec` node gates finish symmetrically to `Ticket`: `workflow_snapshot` resolves spec state (new `resolve_spec_state` on `SessionTicketStateResolver`, default `DefaultTicketStateResolver` reads the sibling `.spec` store), unavailable resolution fails closed, and completion requires a terminal spec state (`verified`/`deprecated`/`cancelled`). `Decision`/`Action`/`Checkpoint` deprecated and deserialize as `Task` via serde aliases (back-compat). CLI/MCP parsers, args, and help updated; taxonomy documented in spec `c677182e` (durable-session-workflow).

Validation: `vt-session-workflow-tooling-fix` / `exec-vt-session-workflow-tooling-fix-20260725` (passed). New session-api tests: `workflow_finish_blocks_spec_node_with_unavailable_state`, `workflow_finish_gates_spec_node_on_terminal_state`, `workflow_finish_ignores_descriptive_category`, `legacy_node_kinds_deserialize_as_task`. `cargo test -p session-api -p session-mcp -p session-cli` green.