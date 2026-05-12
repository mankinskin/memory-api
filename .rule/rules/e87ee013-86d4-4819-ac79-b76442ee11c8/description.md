# rule-mcp

MCP server for `rule-api`.

## Interface

`rule-mcp` runs on stdio and opens the rule store directly. Use it when an agent needs canonical rule CRUD or generated markdown rendering without an HTTP backend.

Named tool groups:

- Authoring: `rule_create`, `rule_get`, `rule_import_file`, `rule_update`, `rule_list`, `rule_search`
- Rendering: `rule_generate_file`, `rule_generate_target`, `rule_explain_target`
- Maintenance: `rule_scan`, `rule_add_root`

Store discovery:

- Set `RULE_INDEX_ROOT` to point at a specific rule store.
- Otherwise the server resolves the nearest `.rule` workspace from the current checkout.

## Usage

Run the server on stdio:

```bash
cargo run -p rule-mcp
```

Example VS Code MCP configuration:

```json
{
  "servers": {
    "rule-mcp": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "-p", "rule-mcp"]
    }
  }
}
```

## Examples

- Call `rule_search` to find existing entries for a README or instruction section.
- Call `rule_explain_target` before changing `rule-targets.yaml` so the client can inspect which canonical entries each node matches.
- Call `rule_generate_target` when a client needs the rendered markdown for one configured output file.
