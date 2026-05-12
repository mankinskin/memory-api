# rule-cli

CLI interface for `rule-api`.

## Interface

Use `rule` when you are authoring canonical rule entries or rendering generated markdown targets from a local checkout.

- `create`, `get`, `update`, `list`, `search`: author and inspect rule entries.
- `import-file`: migrate existing markdown into canonical rule entries.
- `generate-file`, `generate-target`, `explain-target`, `sync-targets`: render deterministic markdown outputs and inspect target composition.
- `scan`, `add-root`: maintain nested workspace discovery and scan roots.

Global options:

- `--json`: emit machine-readable output.
- `--index-root <path>`: override the `.rule` index root.

## Usage

Build or run from a checkout of `memory-viewers/memory-api`:

```bash
cargo build -p rule-cli --bin rule
cargo run -p rule-cli --bin rule -- --help
```

`rule` discovers the nearest `.rule` workspace by walking up from the current directory. Use `--index-root` when you want to point at a different store.

## Examples

```bash
# Search canonical rule entries
rule search "ticket board"

# Inspect a rendered target before regenerating files
rule explain-target --config rule-targets.yaml --target memory-api-readme

# Render one configured target
rule generate-target --config rule-targets.yaml --target rule-cli-readme

# Sync every configured README/instruction target in the repo
rule sync-targets --config rule-targets.yaml
```

Use `import-file` when you are migrating an existing markdown file into canonical rule storage instead of retyping it by hand.
