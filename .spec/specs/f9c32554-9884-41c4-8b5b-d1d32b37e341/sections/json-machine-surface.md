<!-- spec-api:file generated=true -->

<!-- spec-api:entry id=6524f366-dbc2-40f9-84b9-72d948c66de8 slug=memory-api/recurring-principles/json-machine-surface/json-machine-surface/l1 -->
# --json machine surface

The `--json` flag on every `memory-api` CLI selects a stable machine surface. Tooling that consumes JSON output must never need to parse human prose or guess the shape of a list response.

<!-- spec-api:entry id=3ed1f3b8-4ef3-4a21-841c-f2c0339a360a slug=memory-api/recurring-principles/json-machine-surface/json-machine-surface/stable-envelope/l5 -->
## Stable envelope

JSON output is always a single envelope object. The top-level keys are `payload` (on success) or `code`/`message`/`request_id` (on failure), plus an optional `request_id` echoed back at the top level. Commands never emit a bare `[ ... ]` array or a stream of newline-delimited objects.

<!-- spec-api:entry id=c749eef4-2c3d-4652-8356-52a3f8a0b22c slug=memory-api/recurring-principles/json-machine-surface/json-machine-surface/payload-conventions/l9 -->
## Payload conventions

- List responses use `payload.items: [ ... ]` plus `payload.count` and, when applicable, paging cursors.
- Single-entity responses use `payload.<entity>` (for example `payload.ticket`, `payload.spec`, `payload.rule`) carrying the canonical fields the store returns.
- Mutating commands echo `payload.id`, `payload.state`, and (when produced by the call) the canonical folder/path so traceability links can be composed without a follow-up `get`.

<!-- spec-api:entry id=78901a63-64f2-4ea6-a164-2352cd434068 slug=memory-api/recurring-principles/json-machine-surface/json-machine-surface/stability/l15 -->
## Stability

The envelope shape is part of the public contract:

<!-- spec-api:entry id=04879e9c-aa58-4c55-a4cd-f01676403f2c slug=memory-api/recurring-principles/json-machine-surface/json-machine-surface/stability/l19 -->
- Adding fields is allowed.
- Removing or renaming fields requires a version bump and is announced through the spec for the relevant `memory-api` crate.
- `--json` output is the source of truth for tests and for downstream `viewer-ctl`-managed viewers; human output is rendered from the same envelope.
