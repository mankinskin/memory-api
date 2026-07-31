# Typed error envelopes

Every failure from a `memory-api` CLI, MCP tool, or HTTP handler is rendered as a typed JSON envelope. Agents and tooling can branch on `code` without parsing free-form text, and `request_id` lets operators correlate the failure with logs.

## Envelope shape

```json
{
  "code": "<machine-readable category>",
  "message": "<human-readable summary>",
  "request_id": "<uuid>",
  "details": { ... }
}
```

- `code` is one of a small enumerated set (`invalid_request`, `not_found`, `conflict`, `precondition_failed`, `internal_error`, …) shared across all `memory-api` stores.
- `message` is a single human-readable line; it never includes secrets, file contents, or stack traces.
- `request_id` is the propagated `--request-id` or a freshly generated UUID; both CLI/MCP/HTTP surface and the store log emit the same value.
- `details` is optional and may carry structured context (e.g. `{ "id": "abc12345", "state": "ready" }`).

## Where envelopes appear

- CLIs with `--json` emit the envelope as their entire stdout payload on success or failure.
- MCP tools return the envelope inside their result content blocks.
- HTTP responses serialise the envelope as the JSON body with an HTTP status that mirrors `code`.
- Human-readable CLI output (without `--json`) still uses `code` to choose the message and exit code, even when it prints prose.
