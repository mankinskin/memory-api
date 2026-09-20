Every path field in sync-targets output must use `/` separators on all hosts.

Root cause: `sync_target_payload_entry` emits `"output": output` as a raw `PathBuf` (OS separators; backslashes on Windows) and `sync_targets_command` emits `"config": args.config` the same way, while `removed[].output` uses `stable_output_key` (forward-slash normalized) -> mixed separators in one payload.

Files:
- memory-api/tools/cli/rule-cli/src/cli/rendering.rs (sync_target_payload_entry `"output": output`, sync_targets_payload removed entries)
- memory-api/tools/cli/rule-cli/src/cli/dispatch_secondary.rs (sync_targets_command `"config"`, generate_target_command `"output"`, explain_target_command `"output"`)
- memory-api/tools/mcp/rule-mcp/src/server/generate.rs (mirror for MCP consistency)

Plan:
- Add a small `display_path(&Path) -> String` helper that canonicalizes display to forward slashes (reuse the existing `.to_string_lossy().replace('\\', "/")` pattern already in `stable_output_key`).
- Apply it to all path fields emitted by generate-target / sync-targets / explain-target and MCP generate.

Acceptance:
- Unit test asserts no `\\` in any emitted path field given Windows-style input paths.
- generate-target and MCP generate payloads consistent.

Validation: cargo test -p rule-cli; cargo test -p rule-mcp.