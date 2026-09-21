# Problem

`workspace_session_id` is a mandatory parameter on every session-mcp workflow call
(`workflow_add_node`, `add_edge`, `set_status`, `render_*`, `pin`, `unpin`, `view`,
`render_instructions`), yet it is not conveniently returned by `session_runtime_init`.

In session `367ac6a3` (and reproduced again while planning this work), the agent had to spill the
`session_runtime_init` result to a file and run a separate read just to extract the handle before
it could make any workflow call. The most-used value in the entire API is the least convenient to
obtain — a per-session round-trip tax paid on the very first call.

# Scope

1. Make `session_runtime_init` (and `session_runtime_resume`) return `workspace_session_id` as a
   prominent, top-line field in the result payload, clearly labeled as the handle required for
   subsequent workflow calls.
2. Echo `workspace_session_id` back in the result of every session-mcp workflow/runtime tool that
   already requires it, so it is always visible in-band and never has to be re-fetched.
3. Ensure the field is present even when the result is large enough to be spilled to a resource
   file (e.g. include it in a compact header the caller can read without loading the whole blob),
   or keep the init result small enough that it is not spilled.
4. Keep `session-cli` output in parity (print the handle prominently on init/resume).

# Regression Validation Requirements

- **Unit/integration test:** assert `session_runtime_init` / `resume` results include
  `workspace_session_id` as a documented top-line field.
- **Integration test:** assert each workflow/runtime tool result echoes the `workspace_session_id`
  it was called with.
- **Manual/prompt-replay:** confirm an agent can obtain the handle from the init result without a
  separate file read, then immediately make a workflow call.

# Acceptance Criteria

- `session_runtime_init` and `session_runtime_resume` return `workspace_session_id` as a prominent
  top-line field.
- Every session-mcp workflow/runtime tool echoes `workspace_session_id` in its result.
- The handle is obtainable without a separate read of a spilled result file.
- `session-cli` prints the handle prominently on init/resume.

# Likely Surfaces

- `memory-api/tools/mcp/session-mcp/src/server.rs`
- `memory-api/crates/session-api/src/store/config/runtime_workflow.rs`
- `memory-api/tools/cli/session-cli/src/lib.rs`

# Implementation Status — in-review (2026-07-25)

Delivered: new `SessionServer::json_result_with_handle` injects `workspace_session_id` as a top-line field. `session_runtime_init`/`session_runtime_resume` return it top-line (resolved from the result context, so it is present even when the caller passed none), and every workflow/runtime tool (`pin`, `unpin`, `view`, `render_instructions`, `workflow_add_node`/`add_edge`/`set_status`/`promote`, `render_terminal`/`render_mermaid`, `handoff`, `finish`) echoes it. `session-cli` parity via `to_value_with_handle` on init/resume.

Validation: `vt-session-workflow-tooling-fix` / `exec-vt-session-workflow-tooling-fix-20260725` (passed). Test `runtime_init_result_exposes_workspace_session_id_top_line` performs the exact 367ac6a3 replay (`runtime_init` → `workflow_add_node`) and asserts the handle is obtainable inline and echoed. Independent quick win; no dependency on the other four.