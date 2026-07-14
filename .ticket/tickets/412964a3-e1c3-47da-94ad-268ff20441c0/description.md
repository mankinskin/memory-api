# session-api: runtime cognitive-workspace foundation

Extend `session-api` from a write/archive store into the durable read/runtime foundation used by pinned context and the session workflow.

## Decisions baked in

- A durable `workspace_session_id` identifies the logical workspace across handoffs.
- Each agent execution uses a distinct `run_id` with optional predecessor linkage; handoff reuses the workspace ID, not the outgoing run ID.
- Pinned entities are cross-store URNs.
- Context mutations are file-backed and flushed before returning.
- `render_view` returns short headers only; bodies are fetched explicitly.
- No `current_mode`; an empty context is valid.
- Feedback usage/rating emission is optional through an injected sink and cannot fail context mutation.

## Scope

- Add durable runtime context alongside existing capture artifacts without breaking capture/archive behavior.
- Core ops: initialize/resume workspace context, pin, unpin, read context, and render pinned headers.
- Persist workspace/run lineage needed by downstream workflow and handoff slices.
- Focused unit tests, including byte-identity regressions for the existing capture path.

## Dependencies

- Frozen design contract `afa00b5c-c736-4d75-b157-d3e9ce90d819`.
- URN resolver `82d6ada4-ac35-45a7-9df6-7b7501d58e70`.

Feedback-api tickets are no longer hard dependencies. A feedback adapter may be integrated when available, but durable context must work without it.

## Downstream

- Durable workflow persistence `70cd7056-c342-4433-ad60-5bc798f61aa6`.
- CLI/MCP surfaces `6b2dc497-188c-44f5-9106-bf35deecb7a1`.
- Cascade gathering `d8f76965-1ff3-4a0a-bb24-773b9637fae4`.

## Specs

- Runtime context `709f067a-21b6-41b6-8879-3cacef4bacaf`.
- Durable workflow `c677182e-90da-4ac3-8b94-9e2e97c825cf`.