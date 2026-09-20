# Problem

The session-mcp workflow mutation tools accept enum-valued parameters typed as bare `String`,
so the generated JSON schema advertises `"type":"string"` with no `enum` and no description of
legal values. Agents cannot discover valid values from the tool schema.

In session `367ac6a3`, an agent called `session_workflow_add_node` with an invalid `kind`, could
not learn the legal values, and abandoned the durable-workflow feature ("I'll ... rather than
fight the enum"). The session was then interrupted with no durable node to resume from.

# Evidence (source sites)

- `WorkflowAddNodeInput.kind: String`, `WorkflowAddEdgeInput.kind: String`, and the status field
  of the set-status input are bare `String` in `memory-api/tools/mcp/session-mcp/src/server.rs`
  (around lines 167-190), despite the `JsonSchema` derive.
- Legal values already exist as kebab-case enums in
  `memory-api/crates/session-api/src/model/workflow.rs`:
  - `SessionWorkflowNodeKind`: `ticket, action, decision, checkpoint, validation`
  - `SessionWorkflowNodeRequirement`: `required, optional`
  - `SessionWorkflowNodeStatus`: `pending, in-progress, blocked, done, deferred`
  - `SessionWorkflowEdgeKind`: `depends-on, order`

# Scope

1. Make the session-mcp workflow mutation input structs advertise their legal values in the JSON
   schema — either by typing the fields as the real enums, or via `schemars` enum/`description`
   attributes that mirror `session-api` exactly.
2. Cover `WorkflowAddNodeInput.kind`, `WorkflowAddNodeInput.requirement`,
   `WorkflowAddEdgeInput.kind`, and the set-status `status` field.
3. Add short param `description`s stating the meaning and legal values, and keep the accepted
   snake_case/kebab-case aliases the parsers already allow (e.g. `depends-on`/`depends_on`).
4. Keep parity with `session-cli` argument documentation.

# Relationship to the runtime error contract

The runtime rejection message that lists allowed values on an invalid value is owned by
`8bb97b73` (the invalid-transition/enum recovery contract). This ticket owns the *schema-level*
advertisement so the values are discoverable before a call is ever made; together they deliver the
"full contract" for the enum-discoverability fix.

# Regression Validation Requirements

- **Unit/schema test:** assert the generated JSON schema for each workflow mutation tool contains
  the expected `enum` (or documented legal values) for the affected fields.
- **Prompt-replay:** replay the session 367ac6a3 flow (`runtime_init -> workflow_add_node`) and
  confirm an agent can read the legal `kind` values from the tool schema and complete a durable node.
- **Parity:** confirm the advertised values exactly match the `session-api` enums (no drift).

# Acceptance Criteria

- The session-mcp workflow mutation tool schemas advertise the legal values for `kind`,
  `requirement`, edge `kind`, and `status`.
- The advertised values match the `session-api` enums exactly.
- A prompt-replay of the 367ac6a3 flow no longer requires guessing or source-diving to find a legal `kind`.

# Likely Surfaces

- `memory-api/tools/mcp/session-mcp/src/server.rs`
- `memory-api/crates/session-api/src/model/workflow.rs`
- `memory-api/tools/cli/session-cli/src/lib.rs`

# Implementation Status — in-review (2026-07-25)

Delivered: the workflow mutation input fields stay `String` (so `parse_*` keeps accepting snake/kebab aliases and returns the allowed-values error owned by `8bb97b73`), but each now carries `#[schemars(with = ...)]` projecting a helper enum into the generated JSON schema — `WorkflowNodeKindSchema` {ticket, validation, spec, task}, `WorkflowRequirementSchema`, `WorkflowEdgeKindSchema`, `WorkflowNodeStatusSchema`. Param descriptions state the legal values. `session-cli` arg help updated in parity. The advertised set matches the (updated by `203248cb`) `session-api` enums; drift is guarded by a parity test.

Validation: `vt-session-workflow-tooling-fix` / `exec-vt-session-workflow-tooling-fix-20260725` (passed). Tests `workflow_add_node_schema_advertises_kind_and_requirement_enums`, `workflow_edge_and_status_schemas_advertise_enums`, `workflow_enum_parity_with_session_api`, and the 367ac6a3-replay `runtime_init_result_exposes_workspace_session_id_top_line`.