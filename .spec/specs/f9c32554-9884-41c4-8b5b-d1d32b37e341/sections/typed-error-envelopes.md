<!-- spec-api:file generated=true -->

<!-- spec-api:entry id=47a022ef-6211-4e49-80b7-58578f92e472 slug=memory-api/recurring-principles/typed-error-envelopes/typed-error-envelopes/l1 -->
# Typed error envelopes

Every failure from a `memory-api` CLI, MCP tool, or HTTP handler is rendered as a typed JSON envelope. Agents and tooling can branch on `code` without parsing free-form text, and `request_id` lets operators correlate the failure with logs.

<!-- spec-api:entry id=680662fa-c5ce-464b-93e6-4fae449ab598 slug=memory-api/recurring-principles/typed-error-envelopes/typed-error-envelopes/envelope-shape/l5 -->
## Envelope shape

```json
{
  "code": "<machine-readable category>",
  "message": "<human-readable summary>",
  "request_id": "<uuid>",
  "details": { ... }
}
```

<!-- spec-api:entry id=8f20a478-00af-4fa8-a429-625918c6317a slug=memory-api/recurring-principles/typed-error-envelopes/typed-error-envelopes/envelope-shape/l16 -->
- `code` is one of a small enumerated set (`invalid_request`, `not_found`, `conflict`, `precondition_failed`, `internal_error`, …) shared across all `memory-api` stores.
- `message` is a single human-readable line; it never includes secrets, file contents, or stack traces.
- `request_id` is the propagated `--request-id` or a freshly generated UUID; both CLI/MCP/HTTP surface and the store log emit the same value.
- `details` is optional and may carry structured context (e.g. `{ "id": "abc12345", "state": "ready" }`).

<!-- spec-api:entry id=64f91988-d190-4753-8c61-cd8e0515491f slug=memory-api/recurring-principles/typed-error-envelopes/typed-error-envelopes/where-envelopes-appear/l21 -->
## Where envelopes appear

- CLIs with `--json` emit the envelope as their entire stdout payload on success or failure.
- MCP tools return the envelope inside their result content blocks.
- HTTP responses serialise the envelope as the JSON body with an HTTP status that mirrors `code`.
- Human-readable CLI output (without `--json`) still uses `code` to choose the message and exit code, even when it prints prose.
