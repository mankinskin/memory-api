## Tool Use Examples

### Install the CLI tools

From the `context-engine` repo root, the shared installer can run the same four Cargo installs for you:

```bash
bash ./install-tools.sh --tool rule-cli --tool spec-cli --tool ticket-cli --tool audit-cli
```

If you are working directly from a `memory-viewers/memory-api` checkout instead of the repo root, run the underlying install commands from this workspace:

```bash
cargo install --path tools/cli/rule-cli --bin rule
cargo install --path tools/cli/spec-cli --bin spec
cargo install --path tools/cli/ticket-cli --bin ticket
cargo install --path tools/cli/audit-cli --bin audit
```

After that, verify the install with `rule --help`, `spec --help`, `ticket --help`, and `audit --help`.

### Set up a workspace repository

From the repository root, just run the tools. They discover an existing `.rule`, `.spec`, or `.ticket` by walking up from the current directory, and if none exists yet they initialize a local one in the current repository and seed a folder-local `.gitignore` for generated index files.

```bash
rule list
spec list
ticket board show
audit run .
```

When you add the first rule, spec, or ticket, the canonical `rules/`, `specs/`, and `tickets/` folders are created automatically inside those local tool roots.

### Common repo-local tasks

```bash
rule sync-targets --config rule-targets.yaml
spec refs <spec-id> validate
ticket board show
audit run .
```

- Regenerate repo docs from canonical rule content managed by `rule-api`.
- Validate a specification's code references through `spec-api` tooling.
- Inspect active board state through the `ticket-api` command surface.
- Run repository-level audits through `audit-api`.
