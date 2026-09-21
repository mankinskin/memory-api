## Objective

Scaffold the `pdf-api` internal crate and `pdf` public facade crate per `WORKFLOW_TOOLS_DOMAIN_CRATE_CONTRACT.md`, with feature-gated `cli`/`mcp` binaries and root workspace wiring. No domain logic in this ticket — types/logic land in T2+.

## Target Files

- `memory-api/crates/pdf-api/Cargo.toml`
- `memory-api/crates/pdf-api/src/lib.rs` (empty/minimal module skeleton; real types added in T2)
- `memory-api/crates/pdf/Cargo.toml`
- `memory-api/crates/pdf/src/lib.rs` (re-exports `pdf_api::*`)
- `memory-api/crates/pdf/src/bin/pdf-cli.rs` (skeleton entry point only, feature-gated)
- `memory-api/crates/pdf/src/bin/pdf-mcp.rs` (skeleton entry point only, feature-gated)
- `Cargo.toml` (root workspace `members` array — add both new crate paths)

## Design

Mirror `workflow-tools-contract-reference/crates/example-api/Cargo.toml` and `workflow-tools-contract-reference/crates/example/Cargo.toml` exactly, substituting `pdf`/`pdf-api` for `example`/`example-api`, and omitting the `http` feature/bin entirely (v1 excludes HTTP — see the backlogged follow-up ticket).

`memory-api/crates/pdf/Cargo.toml`:
```toml
[package]
name = "pdf"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
path = "src/lib.rs"

[features]
default = []
cli = ["dep:transport-harness", "transport-harness/cli"]
mcp = ["dep:transport-harness", "transport-harness/mcp"]

[[bin]]
name = "pdf-cli"
path = "src/bin/pdf-cli.rs"
required-features = ["cli"]

[[bin]]
name = "pdf-mcp"
path = "src/bin/pdf-mcp.rs"
required-features = ["mcp"]

[dependencies]
pdf-api = { path = "../pdf-api" }
transport-harness = { git = "https://github.com/mankinskin/memory-kernel", branch = "main", optional = true, default-features = false }
```

`memory-api/crates/pdf-api/Cargo.toml` follows the shape of `memory-api/crates/peek-api/Cargo.toml` (a plain library crate with `serde`, `serde_json`, `thiserror` — exact PDF-handling dependency entries/versions are added in T2 per T0's verified crate selection, not in this ticket).

Both `memory-api/crates/pdf-api` and `memory-api/crates/pdf` must be appended to the root `Cargo.toml` `members` array (alongside the existing `memory-api/crates/*` entries).

`pdf-cli.rs` and `pdf-mcp.rs` skeletons should compile and print a placeholder (e.g. "pdf-cli: not yet implemented") — no transport-harness wiring is required to be functional yet, only present and feature-gated correctly per the contract. Full wiring happens in T6/T7.

Do not create any implementation logic for PDF processing in this ticket. Do not add `lopdf`/`pdf-extract`/`printpdf` dependencies yet — that is T2's job once bound to T0's findings.

## Acceptance Criteria

- [ ] `memory-api/crates/pdf-api/` and `memory-api/crates/pdf/` exist with the file layout above.
- [ ] `cargo build -p pdf-api` succeeds.
- [ ] `cargo build -p pdf` succeeds with default (no) features and produces only the library artifact.
- [ ] `cargo build -p pdf --features cli` produces a `pdf-cli` binary.
- [ ] `cargo build -p pdf --features mcp` produces a `pdf-mcp` binary.
- [ ] Both crate paths appear in the root `Cargo.toml` `members` array.
- [ ] `pdf/src/lib.rs` re-exports the public surface of `pdf-api` (e.g. `pub use pdf_api::*;`).
- [ ] No `[[bin]]` target for an `http` feature exists in `memory-api/crates/pdf/Cargo.toml`.
- [ ] No PDF-processing crate (`lopdf`, `pdf-extract`, `printpdf`, etc.) is added as a dependency in this ticket.

## Validation Plan

```bash
cargo build -p pdf-api
cargo build -p pdf
cargo build -p pdf --features cli
cargo build -p pdf --features mcp
```
All four builds must succeed with no warnings about missing bin targets.