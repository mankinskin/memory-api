Phase B. Extract the doc tool into its own `doc` repository (owner mankinskin), built as a single `doc` domain crate per contract `0da6894c`: the crate lib re-exports the internal `doc-api` crate and exposes transports as FEATURE-GATED binary targets (`doc`, `doc-mcp`, and `doc-http` if present) built on the shared `transport-harness` (`dbe0e955`). Frontend `doc-viewer` stays a separate crate.

Follow the parent-tracker recipe (`858c5286`); migrate doc-scoped artifacts via the cross-store move tooling.

## Acceptance criteria
- `doc` builds independently: domain crate lib (primary) re-exporting internal `doc-api` + feature-gated transport bins: bare `doc` CLI plus `doc-mcp` and `doc-http` if present over the harness.
- transport bin smoke pass; doc-viewer browser-verified (screenshot + Playwright).
- doc-scoped artifacts migrated with reference integrity.
- Registered as a workflow-tools dependency.