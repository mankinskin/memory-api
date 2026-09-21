## Objective

Backlog follow-up: add an `http` feature/bin to the `pdf` facade crate, following the same `transport-harness` HTTP wiring pattern used by other domain crates (e.g. `workflow-tools-contract-reference/crates/example`'s `http` feature/`example-http` bin), exposing `pdf-api` operations over HTTP. Explicitly out of the v1 critical path — this ticket is backlog only and has no `depends_on` relationship gating v1 completion.

## Target Files

- `memory-api/crates/pdf/Cargo.toml` (add `http` feature + `pdf-http` `[[bin]]` target, gated `transport-harness/http`)
- `memory-api/crates/pdf/src/bin/pdf-http.rs` (new)

## Design

Mirror `workflow-tools-contract-reference/crates/example/Cargo.toml`'s `http` feature and `example-http` bin exactly, substituting `pdf`/`pdf-http`. Route HTTP handlers to the same `pdf_api::execute` dispatch function used by `pdf-cli` (T6) and `pdf-mcp` (T7), following whatever HTTP adapter convention `transport-harness` establishes (consult an existing `*-http` crate in this repo, e.g. `memory-api/tools/http/doc-http`, for the established request/response/error-mapping pattern to mirror).

This ticket is intentionally deferred: it should not be started until after the v1 epic (T0-T9) is complete and reviewed, since v1 explicitly excludes HTTP transport per the epic's non-goals.

## Acceptance Criteria

- [ ] `memory-api/crates/pdf/Cargo.toml` gains an `http` feature gating `dep:transport-harness`/`transport-harness/http` and a `pdf-http` `[[bin]]` target with `required-features = ["http"]`.
- [ ] `cargo build -p pdf --features http` succeeds and produces a working `pdf-http` binary exposing PDF operations over HTTP.
- [ ] HTTP request/response bodies map to the same `PdfRequest`/`PdfResponse`/`PdfError` types used by the CLI and MCP transports (no duplicated logic).
- [ ] Sandboxing (confinement root) and write-safety (explicit output + overwrite) contracts are enforced identically to the CLI/MCP transports.

## Validation Plan

```bash
cargo build -p pdf --features http
```
Manual HTTP smoke test (e.g. `curl`) against at least the text-extraction and merge endpoints once implemented, confirming behavior matches the CLI/MCP transports for equivalent inputs.