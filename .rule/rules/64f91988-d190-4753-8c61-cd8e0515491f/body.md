## Where envelopes appear

- CLIs with `--json` emit the envelope as their entire stdout payload on success or failure.
- MCP tools return the envelope inside their result content blocks.
- HTTP responses serialise the envelope as the JSON body with an HTTP status that mirrors `code`.
- Human-readable CLI output (without `--json`) still uses `code` to choose the message and exit code, even when it prints prose.