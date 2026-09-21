# Problem

The ticket/spec/rule/session tool surfaces are not self-describing enough for operators or agents.

Sessions spend a disproportionate amount of effort on capability discovery instead of task execution:

- 17 `tool_search` calls
- 14 MCP ticket executions
- 0 direct MCP spec executions
- 0 direct rule-tool executions

The repeated search phrases were about basic discovery (`ticket information access`, `activate
ticket information access bundle`, `spec tools`, `board tools`), not advanced functionality. That
is a sign that the surfaces do not advertise the common workflows clearly enough.

**Added scope — session-mcp.** In session `367ac6a3` an agent ran the
`session_runtime_init -> session_workflow_add_node` durable-planning flow, could not discover the
legal node `kind` values, and abandoned the feature. session-mcp is currently invisible in normal
usage: there is no catalog entry pointing agents at the canonical init/pin/view/workflow flow, its
enum-valued parameters, or the fact that a session handle is required for every workflow call.

# Session Evidence

- The session repeatedly searched for ticket information access and board/state tools before falling back to CLI.
- The session never used direct MCP spec tools despite heavy spec work.
- Rule tooling was effectively absent from real task execution even though rule-related tickets were in scope.
- session-mcp's durable-workflow flow was attempted, undiscoverable, and abandoned (session 367ac6a3).

# Scope

1. Add a self-describing capability catalog / help surface for ticket/spec/rule/**session** workflows.
2. Cover, at minimum:
   - common read flows
   - mutation flows
   - board / next / why-not flows
   - validation flows
   - **session lifecycle flows**: `runtime_init` -> `pin`/`view` -> `workflow_add_node`/`add_edge`/`set_status`
     -> `render_*` -> `handoff`/`finish`, including the enum-valued parameters and their legal values
   - nested-root / nested-store targeting support
3. Expose a machine-readable form for MCP/agent consumers and a human-readable form for CLI/operators.
4. Ensure the catalog points to the canonical command/tool for a workflow instead of requiring semantic-search roulette.
5. Document known parity gaps explicitly so agents do not waste time discovering that a needed surface does not exist yet.

# Regression Validation Requirements

- **Specification / docs:** define the capability-catalog contract and the minimum workflow categories it must describe.
- **MCP / CLI:** add tests showing the help/catalog surface lists the same core workflows and targeting semantics.
- **Rule discoverability:** include at least one rule-oriented workflow in the catalog so rule tooling is not invisible in normal usage.
- **Session discoverability:** include the session lifecycle flow in the catalog; assert the session workflow enums
  are reachable from the catalog rather than only from source.
- **Operator validation:** replay the capability-discovery portion of sessions (including the 367ac6a3 session flow)
  and confirm the common ticket/spec/rule/session workflows can be found without repeated exploratory `tool_search` loops.

# Acceptance Criteria

- One command/tool can list the canonical ticket/spec/rule/session workflows and the parameters they require.
- The catalog explicitly states whether a workflow supports nested roots/stores.
- The catalog lists the session lifecycle flow and its enum-valued parameters with legal values.
- MCP and CLI help surfaces agree on the named workflows and targeting semantics.
- Rule-oriented and session-oriented workflows are discoverable from the same catalog rather than relying on ambient docs only.
- The documented parity gaps are explicit enough that agents can choose the right fallback immediately.

# Likely Surfaces

- `tools/ticket-cli/`
- `tools/ticket-mcp/`
- `tools/spec-cli/`
- `tools/spec-mcp/`
- `crates/rule-api/`
- `memory-api/tools/mcp/session-mcp/`
- `memory-api/tools/cli/session-cli/`
- `README.md`
- `.agents/instructions/`

# Implementation Status — in-review (2026-07-25)

Delivered (session-mcp scope, the added scope of this ticket): new machine-readable `session_capabilities` tool in `memory-api/tools/mcp/session-mcp/src/server.rs` returns a self-describing catalog listing the durable-session lifecycle flow (`runtime_init` → `pin`/`view` → `workflow_add_node`/`add_edge`/`set_status` → `render_terminal`/`render_mermaid` → `handoff`/`finish`), the ordered steps and their canonical tool names, the `workspace_session_id` handle contract, `nested_roots_supported: true`, and every enum-valued workflow parameter with its legal values (behavioral vs descriptive kinds, requirement, edge kind, status). The server `instructions` header now points agents at `session_capabilities`.

Validation: `vt-session-workflow-tooling-fix` / `exec-vt-session-workflow-tooling-fix-20260725` (passed). Test `capabilities_lists_session_lifecycle_and_enums` asserts the lifecycle steps and behavioral-kind enum are reachable from the catalog.

REVIEWER NOTE — remaining original-scope (NOT delivered in this session): the broader ticket/spec/rule capability catalog and CLI/human-readable + MCP parity across those surfaces was out of scope for this session, which was chartered to extend this ticket for the session-mcp surface. Confirm the ticket/spec/rule catalog portion before closing beyond in-review.