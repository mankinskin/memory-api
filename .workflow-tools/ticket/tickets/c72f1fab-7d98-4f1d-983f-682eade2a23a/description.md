# T1: Crate Scaffolding — `pdf-api` + `pdf` Facade

## Objective

Create the two-crate skeleton mandated by
`WORKFLOW_TOOLS_DOMAIN_CRATE_CONTRACT.md`, wired into the root workspace, with
feature-gated transport binaries that build but do nothing yet.

## Files To Create

```
memory-api/crates/pdf-api/Cargo.toml
memory-api/crates/pdf-api/src/lib.rs
memory-api/crates/pdf/Cargo.toml
memory-api/crates/pdf/src/lib.rs
memory-api/crates/pdf/src/bin/pdf-cli.rs
memory-api/crates/pdf/src/bin/pdf-mcp.rs
```

## Files To Modify

- Root `Cargo.toml` — add `"memory-api/crates/pdf-api"` and
  `"memory-api/crates/pdf"` to `members`, adjacent to the existing
  `memory-api/crates/peek-api` / `fs-api` entries.

## Design

### Copy target

`memory-api/crates/ticket/Cargo.toml` is the in-repo crate that already
implements this contract shape exactly. Mirror it, dropping the `http` feature
and bin (decision 10 — out of scope for v1).

### `memory-api/crates/pdf/Cargo.toml`

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

Add `serde`/`serde_json`/`tokio` as optional deps gated behind the transport
features only if the harness requires them, matching how
`memory-api/crates/ticket/Cargo.toml` gates its optional deps.

Do **not** add an `http` feature or bin. The follow-up ticket owns that.

### Naming caution

The package name `pdf` collides with the crates.io `pdf` crate that T0 may
recommend as a dependency. If T0 selects crates.io `pdf`, rename the dependency
with a `package = "pdf"` alias (e.g. `pdf_rs = { package = "pdf", ... }`) inside
`pdf-api`. The facade crate keeps the name `pdf` because the contract mandates
`{domain}` naming and the `pdf-cli` / `pdf-mcp` executable names depend on it.

### `pdf/src/lib.rs`

Re-export only:

```rust
pub use pdf_api::*;
```

Transport-agnostic composition only. No logic.

### `pdf-api/src/lib.rs`

Empty placeholder in this ticket — T2 fills it in. Just enough to compile
(crate docs comment + module declarations that T2 will populate).

### Bins

Minimal stubs that compile under their feature and exit cleanly. Real wiring
lands in T6 (`pdf-cli`) and T7 (`pdf-mcp`).

## Acceptance Criteria

- [ ] `cargo build -p pdf` builds a slim library with neither bin.
- [ ] `cargo build -p pdf --features cli` produces an executable named exactly
      `pdf-cli`.
- [ ] `cargo build -p pdf --features mcp` produces an executable named exactly
      `pdf-mcp`.
- [ ] `cargo build -p pdf --features cli,mcp` builds both.
- [ ] `cargo metadata` lists both new crates as workspace members.
- [ ] `pdf::` re-exports the `pdf-api` public surface.
- [ ] No `http` feature or bin exists.
- [ ] `cargo build --workspace` still succeeds — no regression to existing
      crates.

## Validation

```bash
cargo build -p pdf
cargo build -p pdf --features cli,mcp
cargo build --workspace
```

## Depends On

T0 — the dependency set must be known before `pdf-api/Cargo.toml` is written.
