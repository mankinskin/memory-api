## What lives in the adapter crates

- `<x>-cli`: clap argument parsing, JSON envelope rendering, exit-code mapping.
- `<x>-mcp`: tool descriptors that delegate to `<x>-api`.
- `<x>-http`: route handlers that delegate to `<x>-api` and serialise the envelope.
- `viewer-ctl`-managed viewers: read-mostly consumers that talk to `<x>-http` and never duplicate model logic.