## Tool Use Examples

```bash
cargo run -p rule-cli -- sync-targets --config memory-viewers/memory-api/rule-targets.yaml
cargo run -p ticket-cli -- board show
cargo run -p spec-cli -- refs <spec-id> validate
cargo run -p audit-cli -- help
```

- Regenerate the repo README from canonical rule content managed by `rule-api`.
- Inspect active board state through the `ticket-api` command surface.
- Validate a specification's code references through `spec-api` tooling.
- Discover the available review and audit flows exposed by `audit-api`.
