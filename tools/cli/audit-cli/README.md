<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=011df0af-b945-434a-b681-a3949de76ec0 slug=memory-api/readme/tools/cli/audit-cli/l1 -->
# audit-cli

CLI interface for `audit-api`.

## Interface

Use `audit` when you need repository quality metrics and grouped findings from the terminal.

- `run`: execute a full audit and return metrics, thresholds, and actionable findings.
- `summary`: regroup findings by crate, category, severity, metric, or path.

Global options:

- `--json`: emit machine-readable output instead of the human summary.

## Usage

Build or run from a checkout of `memory-viewers/memory-api`:

```bash
cargo build -p audit-cli --bin audit
cargo run -p audit-cli --bin audit -- --help
```

Run the command from the repository you want to audit, or pass an explicit repo path to the subcommand.

## Examples

```bash
# Run the default audit for the current repository
audit run .

# Tighten thresholds for a focused cleanup pass
audit run . --max-file-lines 300 --max-cyclomatic-complexity 10 --coverage-warn-below 85

# Group findings by severity
audit summary --by severity .
```
