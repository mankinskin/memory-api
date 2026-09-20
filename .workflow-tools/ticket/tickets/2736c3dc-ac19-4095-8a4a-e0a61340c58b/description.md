Phase B. Extract the log tool into its own `log` repository (owner mankinskin), built as a single `log` domain crate per contract `0da6894c`: the crate lib re-exports the internal `log-api` crate and exposes transports as FEATURE-GATED binary targets (`log`, `log-mcp`, and `log-http` if present) built on the shared `transport-harness` (`dbe0e955`). Frontend `log-viewer` stays a separate crate.

Follow the parent-tracker recipe (`858c5286`); migrate log-scoped artifacts via the cross-store move tooling. Coordinate with in-flight log-viewer Dioxus port tickets.

## Acceptance criteria
- `log` builds independently: domain crate lib (primary) re-exporting internal `log-api` + feature-gated transport bins: bare `log` CLI plus `log-mcp` and `log-http` if present over the harness.
- transport bin smoke pass; log-viewer browser-verified (screenshot + Playwright).
- log-scoped artifacts migrated with reference integrity.
- Registered as a workflow-tools dependency.