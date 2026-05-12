# audit-mcp

MCP server for `audit-api`.

## Interface

`audit-mcp` runs on stdio and computes repository audit results directly from the local checkout.

Named tools:

- `audit`: run the full repository audit and return the structured `AuditReport` payload.
- `audit_summary`: run the audit and regroup findings by crate, category, severity, metric, or path.

Each call accepts the repository root plus optional threshold overrides such as `max_file_lines`, `max_cyclomatic_complexity`, and `coverage_warn_below`.

## Usage

Run the server on stdio:

```bash
cargo run -p audit-mcp
```

Example VS Code MCP configuration:

```json
{
  "servers": {
    "audit-mcp": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "-p", "audit-mcp"]
    }
  }
}
```

## Examples

- Call `audit` when a client needs the full metrics and findings payload for a repository.
- Call `audit_summary` when a client needs grouped issue counts, for example by severity during triage.
- Override the threshold fields per call when you want a stricter cleanup pass than the default repository policy.
