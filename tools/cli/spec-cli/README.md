<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=df2d4ede-7992-4ae0-9ca5-8afc654bbdd0 slug=memory-api/readme/tools/cli/spec-cli/l1 -->
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
- `--index-root <path>`: override the `.spec` index root.

## Usage

Build or run from a checkout of `memory-viewers/memory-api`:

```bash
cargo build -p spec-cli --bin spec
cargo run -p spec-cli --bin spec -- --help
```

`spec` finds the nearest `.spec` workspace by walking up from the current directory. Use `--index-root` when you want to point at a different store.

## Examples

```bash
# List the current specs
spec list

# Search by title, slug, or body text
spec search "ticket board"

# Validate code references on one spec
spec refs <spec-id> validate

# Show the current section tree
spec tree <spec-id>
```
