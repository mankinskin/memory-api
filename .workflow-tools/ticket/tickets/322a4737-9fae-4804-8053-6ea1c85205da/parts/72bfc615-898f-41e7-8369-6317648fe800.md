# Epic: PDF Domain Capability

## Objective

Add a `pdf` domain to the repository so agents can work with PDF files through
MCP named tools, backed by a pure-Rust implementation, plus a hand-authored
skill that teaches agents when and how to use it.

## Capabilities (v1)

| # | Capability | Ticket |
|---|---|---|
| 1 | Extract text from a PDF | T3 |
| 2 | Merge multiple PDFs into one | T4 |
| 3 | Split a PDF into page ranges / single pages | T4 |
| 4 | Edit pages (reorder, delete, rotate) and read/write document metadata | T4 |
| 5 | Create a new PDF (programmatic primitive + optional typst path) | T5 |
| 6 | Extract embedded raster images | T9 (cuttable) |

## Locked Decisions

These were resolved with the user before the track was authored. Do not re-open
them during implementation; if implementation evidence contradicts one, stop and
escalate rather than silently deviating.

1. **Crate shape — follow the written contract, not the peek precedent.**
   `WORKFLOW_TOOLS_DOMAIN_CRATE_CONTRACT.md` governs. An internal `pdf-api`
   crate holds types/logic/errors. A public facade crate `pdf` re-exports
   `pdf-api` from `src/lib.rs` and declares `[[bin]]` targets
   `pdf-cli` and `pdf-mcp`, each gated by a cargo feature (`cli`, `mcp`) that
   also gates the shared `transport-harness` dependency.
   `memory-api/crates/ticket/Cargo.toml` is the in-repo crate that already
   implements this exact shape and is the closest copy target.
2. **Placement.** `memory-api/crates/pdf-api/` and `memory-api/crates/pdf/`,
   both registered in the ROOT `Cargo.toml` `members` list.
3. **Pure Rust only.** `lopdf` for structure and page operations, `pdf-extract`
   for text, `krilla` for creation (selected after research; `printpdf` was
   evaluated and **dropped** — see T5). No
   `pdfium-render`, no C/C++ bindings — not even behind an optional feature.
   Rationale: reproducible builds and cross-platform CI without a native
   toolchain.
4. **Creation has two modes.** A programmatic primitive, plus an optional path
   that shells out to `typst-cli` **only when it is detected on PATH**. typst is
   not a build dependency; absence must produce a clear, actionable error, never
   a panic or a silent fallback.
5. **Write safety.** Copy-on-write by default: an explicit output path is
   required. In-place overwrite is permitted only when the request carries an
   explicit `overwrite: true`. Never silently clobber an existing file.
   `memory-api/crates/fs-api/src/mutation.rs` already implements this exact
   `overwrite` semantic and is the precedent to mirror.
6. **Sandboxing.** All reads and writes are confined to an allowlisted root,
   reusing `fs-api::security::validate_path_within_root` in
   `memory-api/crates/fs-api/src/security.rs`. That function canonicalizes both
   path and root to defeat symlink/junction escapes, and handles
   not-yet-existing destination paths by canonicalizing the parent. There is no
   opt-out.
7. **Image extraction is sequenced last** (T9) and nothing may depend on it, so
   it can be cut without blocking the rest of the track.
8. **No crate facts are verified.** The session that produced this track had no
   web access. No crate version, API signature, license, or feature flag has
   been confirmed. T0 is a bounded verification spike that must resolve this
   before any dependency is added. Downstream tickets therefore state
   acceptance criteria **behaviorally**, not in terms of specific upstream APIs.
9. **Skill is hand-authored.** `.agents/skills/pdf/SKILL.md`, plain markdown,
   NOT rule-mcp generated, and must not carry a `rule-api:file generated=true`
   header. It directs agents to the `pdf-mcp` named tools and documents
   `pdf-cli` as the fallback, mirroring how `peek` is documented.
10. **HTTP transport is out of scope for v1.** Tracked as a separate backlog
    follow-up ticket, not on the v1 critical path.

## Non-Goals

- No `pdf-http` bin in v1.
- No native/C++ bindings.
- No viewer or frontend crate.
- No OCR, no scanned-document text recovery.
- No PDF form filling, signing, or encryption in v1.

## Architecture

Follows the `peek-api` shape for the API crate:

- A tagged serde request enum (`#[serde(tag = "kind")]`) covering every
  operation, response types, and a `PdfError` (`thiserror`) enum.
- One `execute(&PdfRequest) -> Result<PdfResponse, PdfError>` dispatch entry.
- All bounding, validation, and safety enforcement live in `pdf-api`. Transport
  layers stay thin.

MCP surface follows `memory-api/tools/mcp/peek-mcp/src/server.rs`: one
`#[derive(Deserialize, JsonSchema)]` input struct per named tool, a
`ToolRouter`-carrying server struct, a `json_result` helper, and an error mapper
sending user errors to `McpError::invalid_params` and everything else to
`internal_error`.

## Sequencing

T0 → T1 → T2 → {T3, T4, T5} → T6 → T7 → T8, with T9 last and unblocking nothing.

## Risk Register

| Risk | Mitigation |
|---|---|
| Candidate crates may not actually cover the capabilities | T0 spike gates all dependency choices |
| A candidate crate may carry an incompatible license | T0 must confirm MIT/Apache-2.0 compatibility |
| Image extraction may need a decoder stack we do not want | T9 is sequenced last and is cuttable |
| Agent-invoked file writes are a data-loss vector | Decisions 5 + 6, enforced centrally in T2 |
