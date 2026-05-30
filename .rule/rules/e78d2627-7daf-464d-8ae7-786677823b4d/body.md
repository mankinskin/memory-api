## Validation and Operator Notes

- Favor the nearest executable check for the surface you are changing.
- For the nested rule workspace slice, focused validation currently includes `cargo test -p rule-cli generate_target_collects_rules_from_nested_workspaces -- --nocapture`.
- Local authoring remains covered by `cargo test -p rule-api create_defaults_to_local_rules_root_even_with_extra_scan_roots -- --nocapture`.
- MCP parity for nested workspace discovery is compile-checked with `cargo check -p rule-mcp`.
- Repo-local README targets should pass `rule sync-targets --config rule-targets.yaml --check` before review.
