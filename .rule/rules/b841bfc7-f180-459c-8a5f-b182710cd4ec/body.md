# rule-cli

CLI interface for `rule-api`.

## Interface

Use `rule` when you are authoring canonical rule entries, recording feedback on generated guidance, or rendering generated markdown targets from a local checkout.

- `create`, `get`, `update`, `feedback`, `list`, `search`: author, review, and annotate rule entries.
- `import-file`: migrate existing markdown into canonical rule entries.
- `generate-file`, `generate-target`, `explain-target`, `sync-targets`: render deterministic markdown outputs and inspect target composition.
- `scan`, `add-root`: maintain nested workspace discovery and persisted scan roots.

Global options:

- `--json`: emit machine-readable output.
- `--toon`: emit machine-readable TOON output.
- `--index-root <path>`: override the `.rule` index root.
- `--workspace-root <path>`: target a nested workspace repo root and normalize it to the owning `.rule` store.

## Usage

Build or run from a checkout of `memory-viewers/memory-api`:

```bash
cargo build -p rule-cli --bin rule
cargo run -p rule-cli --bin rule -- --help
```

`rule` discovers the nearest `.rule` workspace by walking up from the current directory. Use `--index-root` when you want to point at a different store, or `--workspace-root` when you want to target a nested workspace repo root from an ancestor checkout.

When you want compact structured output for scripts or agent workflows, prefer `rtk rule --toon ...` over `rtk rule --json ...` and decode TOON with `toon-format` / `toon-rust`.

Read and render commands use the scan roots that are already persisted in the active store; they do not walk descendant workspaces automatically on every invocation. Run `rule scan` when you want to discover child `.rule` stores from the active workspace and persist those roots for later `get`, `list`, `search`, `generate-target`, `explain-target`, or `sync-targets` runs.

`rule scan` reports the active scan roots it used, the number of entity folders it integrated into the index/search stores, the number of stale indexed entities it pruned during a reindex, and the full list of manifest parse diagnostics with `path` and `reason` fields.

Target configs can include `imports:` entries that point at either specific config files or `rule-targets/` directories of themed fragments. Imported targets keep their own config-relative output roots, so a parent `sync-targets` run can reuse child target definitions without copying them into the parent config, and top-level `rule-targets.yaml` files can stay as thin import shims over those themed directories.

Feedback is rule-entry scoped. If you are reacting to a specific spec entry or generated instruction section, first resolve the canonical rule entry that produced the text, then carry the spec ID, path, and section in the feedback note.

## Examples

```bash
# Search canonical rule entries
rule search "ticket board"

# Emit compact machine-readable output
rtk rule --toon search "ticket board"

# Discover child workspaces once and reuse the persisted scan roots
rule scan

# Search the nested memory-api rule store from the root checkout
rule --workspace-root memory-viewers/memory-api search "workspace root"

# Record feedback for a rule entry tied to a spec section
rule feedback shared/agent-rules/quality-gates/l42 \
  --rating mixed \
  --note "Spec target: auth/login :: Rate limits. Needs a concrete edge-case example." \
  --note-kind suggestion \
  --session-id session-42 \
  --agent-or-user-id copilot-gpt-5.4

# Find low-rated or unresolved entries
rule list --low-rated-only
rule list --unresolved-only

# Inspect a rendered target before regenerating files
rule explain-target --config rule-targets.yaml --target memory-api-readme

# Render one configured target
rule generate-target --config rule-targets.yaml --target rule-cli-readme

# Sync every configured target in the repo, including imported child configs
rule sync-targets --config rule-targets.yaml
```

Use `import-file` when you are migrating an existing markdown file into canonical rule storage instead of retyping it by hand.
