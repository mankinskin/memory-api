## Objective

Add a new `pdf` domain capability so agents can, via MCP named tools (and CLI fallback):
1. Extract text from PDFs
2. Extract embedded images from PDFs
3. Edit existing PDFs (page operations: merge/split/reorder/delete; metadata read+write)
4. Create new PDFs (programmatic primitive + optional typst-cli path)
5. Merge/split PDFs

Ship a companion hand-authored skill at `.agents/skills/pdf/SKILL.md`.

## Scope

In scope: `pdf-api` internal crate, `pdf` public facade crate with `pdf-cli` and `pdf-mcp` binaries, root-confined sandboxed I/O, copy-on-write write safety, the six capabilities above, and the skill doc.

Out of scope (non-goals):
- No `pdf-http` bin/feature for v1 (tracked as a separate backlog ticket, not on the critical path).
- No native/C/C++ PDF bindings (no `pdfium-render` or similar), pure-Rust crates only.
- No viewer/frontend UI for PDFs.
- No OCR (scanned-image-to-text) capability.
- No hard build-time dependency on `typst-cli`; it is an optional runtime-detected path.

## Locked Decisions (user-approved this session — do not re-open)

1. **Crate shape** follows `WORKFLOW_TOOLS_DOMAIN_CRATE_CONTRACT.md` exactly (not the peek precedent): an internal `pdf-api` crate holding types/logic/errors, plus a public facade crate `pdf` whose `lib.rs` re-exports `pdf-api` and declares `src/bin/pdf-cli.rs` and `src/bin/pdf-mcp.rs` as `[[bin]]` targets gated by cargo features `cli` and `mcp` (each gating the shared `transport-harness` dependency from `memory-kernel/crates/transport-harness`). Executable names must be exactly `pdf-cli` and `pdf-mcp`. Reference: `workflow-tools-contract-reference/crates/example-api` + `workflow-tools-contract-reference/crates/example`.
2. **Placement**: `memory-api/crates/pdf-api/` and `memory-api/crates/pdf/`. Both must be added to the ROOT `Cargo.toml` `members` list.
3. **Dependency policy**: pure Rust only — `lopdf` / `pdf-extract` / `printpdf` family. NO `pdfium-render`, no C/C++ bindings, not even behind a feature flag.
4. **Creation** has two modes: a programmatic primitive (printpdf-style) AND an optional typst path that shells out to `typst-cli` only when detected on PATH, degrading gracefully with a clear error when absent. `typst-cli` is NOT a hard build dependency.
5. **Write safety**: copy-on-write by default (explicit output path required); in-place overwrite permitted only when the request carries an explicit `overwrite: true` flag. Never silently clobber.
6. **Sandboxing**: reads and writes confined to an allowlisted root, reusing the `fs-api` path-allowlist / root-confinement precedent — specifically `validate_path_within_root` in `memory-api/crates/fs-api/src/security.rs`.
7. **Image extraction** is in scope for v1 but sequenced LAST (T9) so it can be cut without blocking anything else. Nothing else may depend on it.
8. **Unverified crate facts**: no web access was available during ticket authoring, so no crate version, API, license, or feature flag has been verified. T0 is a bounded verification spike that confirms, per candidate crate: latest version, maintenance status, license (must be MIT/Apache-2.0 compatible), and which of the six capabilities it covers. All downstream tickets bind to whatever T0 concludes; acceptance criteria in T2+ are written behaviorally, not against hard-coded unverified API signatures.
9. **Skill**: hand-authored `.agents/skills/pdf/SKILL.md` (plain markdown, NOT rule-mcp generated, must NOT carry a `rule-api:file generated=true` header). Required frontmatter: `name` + `description`. Instructs agents to prefer `pdf-mcp` named tools, documents `pdf-cli` as fallback, mirroring how peek is documented. Add a row to the Master Index table in `.agents/skills/README.md`.
10. **HTTP transport** is OUT of scope for v1. One separate follow-up ticket for the `http` feature/bin exists in the backlog, not part of the v1 critical path.

## Capability List (v1)

1. Text extraction
2. Embedded image extraction (last, cuttable)
3. Page operations: merge, split, reorder, delete
4. Metadata read + write
5. PDF creation: programmatic primitive
6. PDF creation: optional typst-cli path

## Reference Implementations to Study

- `memory-api/tools/mcp/peek-mcp/src/server.rs` — canonical named-tool MCP pattern (per-tool Deserialize+JsonSchema input struct, `ToolRouter`, `json_result`, error-mapper to `McpError`, `#[tool_router]`/`#[tool_handler]`, `run_mcp_server()` via `stdio()`).
- `memory-api/crates/peek-api/src/lib.rs` — canonical api-crate shape (tagged serde request enum, single `execute(&Request) -> Result<Response, Error>` dispatch, `thiserror` error enum, output bounding lives in the api crate).
- `memory-api/crates/fs-api/src/security.rs` — `validate_path_within_root` root-confinement precedent to reuse for pdf-api sandboxing.
- `workflow-tools-contract-reference/crates/example-api` and `.../example` — compiling reference for the api+facade crate shape and feature-gated `[[bin]]` wiring.

## Child Tickets

See `depends_on` edges in the ticket graph for execution order. Order: T0 → T1 → T2 → T3, T4, T5 (can parallelize after T2) → T6, T7 (need T2 and their respective capability tickets) → T8 → T9 (last, no dependents). The HTTP follow-up ticket is backlogged and not part of the T0-T9 chain.

## Health

Ticket health check (`mcp_ticket-mcp_health_check`) must pass with no findings for this epic and all child tickets before work begins.