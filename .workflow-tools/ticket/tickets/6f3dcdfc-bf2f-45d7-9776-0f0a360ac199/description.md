# [test-cli] Add test-result store and `test` CLI for validation evidence

`test-api` currently only defines validation identities (`ValidationSpec`, `ValidationExecution`) with no persistence and no CLI. Validation results are being written verbatim into ticket descriptions, which bloats tickets and provides no queryable evidence store.

## Goal

- Add a persistent `TestStore` to `test-api` (file-backed, mirroring the `.ticket`/`.spec` store conventions) that records `ValidationSpec` and `ValidationExecution` entries under a `.test/<workspace>/` directory.
- Add a thin `test-cli` (`test` binary) following the established CLI conventions (clap-derive, `--json`/`--toon`/text output, `--store-root`/`--workspace-root`/`--workspace-slug` globals) over the store.
- Record the ticket-vscode Rust/WASM parity validation executions in the store instead of inline in the ticket.
- Link the recorded store entries back from ticket `6de424b0` via `ValidationLinks.ticket_ids` and a concise pointer in the ticket description.

## Acceptance criteria

- [x] `test-api` exposes a file-backed store with record/get/list for specs and executions.
- [x] `test` CLI supports recording specs/executions and querying by ticket/spec/outcome with `--json`/`--toon`/text output.
- [x] The crate is wired into the workspace and `cargo test -p test-api` + `cargo test -p test-cli` pass.
- [x] Parity validation results for ticket-vscode are stored as executions and linked from ticket `6de424b0` (which no longer carries the verbose inline result block).

## Validation

- `cargo test -p test-api`: 9 passed, 0 failed
- `cargo test -p test-cli`: 3 passed, 0 failed
- 7 specs recorded under `memory-api/.test/default/specs/`
- 7 executions recorded under `memory-api/.test/default/executions/`, all linked to ticket `6de424b0`
- Verified: `test --store-root memory-api/.test --toon list --ticket 6de424b0-68ec-43c7-9d70-eb8d17305ab3` returns count 7

## Related

- Consumes results from: [6de424b0 Validate Rust/WASM parity](../6de424b0-68ec-43c7-9d70-eb8d17305ab3/ticket.toml)
- Follows native-store-ownership direction in spec a4f48d84 (future test-api responsibilities).