<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=b8535c8a-4097-4042-8f2a-745123d269ee slug=memory-api/readme/tools/parent-readme/l1 -->
Back to [memory-api/README.md](../../../README.md).

<!-- rule-api:entry id=e87ee013-86d4-4819-ac79-b76442ee11c8 slug=memory-api/readme/tools/mcp/rule-mcp/l1 -->
# rule-mcp

MCP server for `rule-api`.

## Interface

`rule-mcp` runs on stdio and opens the rule store directly. Use it when an agent needs canonical rule CRUD, feedback capture, or generated markdown rendering without an HTTP backend.

Named tool groups:

- Authoring: `rule_create`, `rule_get`, `rule_import_file`, `rule_update`, `rule_record_feedback`, `rule_list`, `rule_search`
- Rendering: `rule_generate_file`, `rule_generate_target`, `rule_explain_target`
- Maintenance: `rule_scan`, `rule_add_root`

`rule_update` accepts sparse request payloads. Omit untouched keys entirely; send only the fields, `to_state`, or `body` being changed. The response returns only directly relevant update metadata such as `id`, `changed_fields`, `state_transition`, and `body_updated`.

Store discovery:

- Set `RULE_INDEX_ROOT` to point at a specific rule store.
- Otherwise the server resolves the nearest `.rule` workspace from the current checkout.

Feedback is rule-entry scoped. If the issue came from a specific spec entry or generated guidance section, locate the canonical rule entry first and include the spec ID, path, and section in the feedback note.

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

- Call `rule_search` to find existing entries for a README, instruction section, or generated guidance block.
- Call `rule_record_feedback` to attach a rating and optional note to the exact rule entry you used. When feedback came from a specific spec entry, include the spec ID, path, and section in `note`.
- Call `rule_list` or `rule_search` with `low_rated_only` or `unresolved_only` when the client needs a review queue.
- Call `rule_explain_target` before changing `rule-targets.yaml` so the client can inspect which canonical entries each node matches.
- Call `rule_generate_target` when a client needs the rendered markdown for one configured output file.
- For `rule_update`, prefer sparse payloads such as `{"id":"<id-or-slug>","to_state":"reviewed"}` or `{"id":"<id-or-slug>","field_map":{"order_key":10}}`.
