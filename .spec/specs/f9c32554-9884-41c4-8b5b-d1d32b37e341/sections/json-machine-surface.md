# --json machine surface

The `--json` flag on every `memory-api` CLI selects a stable machine surface. Tooling that consumes JSON output must never need to parse human prose or guess the shape of a list response.

## Stable envelope

JSON output is always a single envelope object. The top-level keys are `payload` (on success) or `code`/`message`/`request_id` (on failure), plus an optional `request_id` echoed back at the top level. Commands never emit a bare `[ ... ]` array or a stream of newline-delimited objects.

## Payload conventions

- List responses use `payload.items: [ ... ]` plus `payload.count` and, when applicable, paging cursors.
- Single-entity responses use `payload.<entity>` (for example `payload.ticket`, `payload.spec`, `payload.rule`) carrying the canonical fields the store returns.
- Mutating commands echo `payload.id`, `payload.state`, and (when produced by the call) the canonical folder/path so traceability links can be composed without a follow-up `get`.

## Stability

The envelope shape is part of the public contract:

- Adding fields is allowed.
- Removing or renaming fields requires a version bump and is announced through the spec for the relevant `memory-api` crate.
- `--json` output is the source of truth for tests and for downstream `viewer-ctl`-managed viewers; human output is rendered from the same envelope.
