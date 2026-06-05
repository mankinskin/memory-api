<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=b8535c8a-4097-4042-8f2a-745123d269ee slug=memory-api/readme/tools/parent-readme/l1 -->
Back to [memory-api/README.md](../../../README.md).

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

`spec_update` accepts sparse request payloads. Omit untouched keys entirely; send only the fields, `to_state`, or `body` being changed. The response returns only directly relevant update metadata such as `id`, `changed_fields`, `state_transition`, and `body_updated`.

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
- For `spec_update`, prefer sparse payloads such as `{"id":"<id-or-slug>","field_map":{"fulfillment_status":"done"}}` or `{"id":"<id-or-slug>","to_state":"reviewed"}`.
