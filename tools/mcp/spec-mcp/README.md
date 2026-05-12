<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=df860347-773a-4489-b37e-b110000b09d7 slug=memory-api/readme/tools/mcp/spec-mcp/l1 -->
# spec-mcp

MCP server for `spec-api`.

## Interface

`spec-mcp` runs on stdio and opens the spec store directly. Use it when an agent needs spec CRUD, tree views, section editing, or code-reference validation without an HTTP backend.

Named tool groups:

- CRUD and discovery: `spec_create`, `spec_get`, `spec_update`, `spec_delete`, `spec_list`, `spec_search`
- Structure and validation: `spec_tree`, `spec_health`, `spec_refs_validate`
- Sections: `spec_section_add`, `spec_section_list`, `spec_section_get`, `spec_section_delete`
- Maintenance: `spec_scan`, `spec_add_root`

Store discovery:

- Set `SPEC_INDEX_ROOT` to point at a specific spec store.
- Otherwise the server falls back to the nearest `.spec` workspace.

## Usage

Run the server on stdio:

```bash
cargo run -p spec-mcp
```

Example VS Code MCP configuration:

```json
{
  "servers": {
    "spec-mcp": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "-p", "spec-mcp"]
    }
  }
}
```

## Examples

- Call `spec_list` when a client needs the current specification inventory.
- Call `spec_tree` to render the section hierarchy for one spec.
- Call `spec_refs_validate` before review to confirm that referenced files and symbols still resolve.
