### Validation status

- Passing: `cargo test -p spec-cli sync_generated -- --nocapture`
- Passing pilot flow: `rule explain-target --config rule-targets.yaml --target spec-api-generated-documents-body --json`, `spec sync-generated 1cf68c36-7f64-4d81-b553-1947b978fbe3 --workspace-root . --json`, `spec get 1cf68c36-7f64-4d81-b553-1947b978fbe3 --full --json`, `spec search "migration-workflow" --limit 5 --json`, and `spec refs 1cf68c36-7f64-4d81-b553-1947b978fbe3 validate --workspace-root . --json`.
- Passing broader suite: `cargo test -p spec-cli`.
