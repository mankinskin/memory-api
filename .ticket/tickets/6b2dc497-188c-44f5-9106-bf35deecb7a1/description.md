# session-cli + session-mcp: durable context and workflow surfaces

Expose runtime context, workflow, rendering, handoff/resume, and finish operations through CLI and MCP.

## CLI scope

Add commands alongside existing capture/query/peek/move surfaces:

- `session init|resume`
- `session pin|unpin|view`
- `session workflow add-ticket|add-step|update|link|promote|show|finish`
- `session workflow show --format terminal|mermaid`
- `session handoff`

Honor compact text, TOON, and JSON output conventions.

## MCP scope

Add structured counterparts for initialization/resume, pinning, workflow mutation, terminal/Mermaid rendering, handoff, and finish.

## Contract

- Inputs distinguish durable `workspace_session_id` from per-run `run_id`.
- Handoff persists before rendering and returns an exact resume command.
- Resume reuses the workspace ID and creates a distinct linked run ID.
- `view` remains headers-only.
- Feedback/rating integration is optional and cannot block context or workflow operations.
- Cascade auto-discovery is not a dependency for manual init/pin/workflow commands; cascade can be added later as an optional init adapter.

## Dependencies

- Runtime context `412964a3-e1c3-47da-94ad-268ff20441c0`.
- Workflow persistence `70cd7056-c342-4433-ad60-5bc798f61aa6`.
- Terminal/Mermaid rendering `cc4b0289-b6fd-412f-a97a-497f05f572f4`.
- Handoff/resume core `0647a212-9d2e-4943-9627-f854ce3f14c4`.

## Specs

- `709f067a-21b6-41b6-8879-3cacef4bacaf`.
- `c677182e-90da-4ac3-8b94-9e2e97c825cf`.