Back to [memory-api/README.md](../../../README.md).

# spec-cli

CLI interface for `spec-api`.

## Interface

Use `spec` when you need to create, browse, and validate specification documents from a local checkout.

- `create`, `get`, `update`, `delete`, `list`, `search`: maintain spec records and inspect them by id, slug, or text query.
- `tree`, `section`: navigate or edit hierarchical spec structure.
- `refs`: list and validate code references attached to a spec.
- `health`, `scan`, `add-root`, `bootstrap`: keep the store healthy and seed specs from Rust API surfaces.

Global options:

- `--json`: return machine-readable output.
- `--toon`: return machine-readable TOON output.
- `--index-root <path>`: override the `.spec` index root.

## Usage

Build or run from a checkout of `memory-viewers/memory-api`:

```bash
cargo build -p spec-cli --bin spec
cargo run -p spec-cli --bin spec -- --help
```

`spec` finds the nearest `.spec` workspace by walking up from the current directory. Use `--index-root` when you want to point at a different store.

For compact structured automation in this repository, prefer `rtk spec --toon ...` over `rtk spec --json ...`. The `--fields-file` option on `create` and `update` accepts either JSON or TOON object payloads, and Rust-side decoding should use `toon-format` / `toon-rust`.

## Examples

```bash
# List the current specs
spec list

# Emit compact structured output
rtk spec --toon list

# Search by title, slug, or body text
spec search "ticket board"

# Apply structured fields from a TOON object file
spec update <spec-id> --fields-file contract-update.toon

# Validate code references on one spec
spec refs <spec-id> validate

# Show the current section tree
spec tree <spec-id>
```
