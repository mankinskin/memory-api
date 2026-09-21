Phase B. Pre-create the `interview` repository (owner mankinskin) as a placeholder for the interview tool, scaffolded per contract `0da6894c`: a single `interview` domain crate skeleton whose lib will re-export the internal `interview-api` crate, with FEATURE-GATED transport bin stubs (`interview`, `interview-mcp`) built on the shared `transport-harness` (`dbe0e955`) and artifact stores. The interview tool is planned but not yet built (see interview-api ticket `7639449a`).

Follow the parent-tracker recipe (`858c5286`) to the extent components exist; otherwise scaffold the single-crate skeleton and register the placeholder in workflow-tools.

## Acceptance criteria
- `interview` repo exists with the single domain-crate skeleton (lib stub + internal api crate stub + feature-gated transport bin stubs: bare `interview` CLI plus `interview-mcp` + stores).
- Registered as a workflow-tools dependency (placeholder allowed).
- Linked to the interview-api design ticket so implementation targets this repo and layout.