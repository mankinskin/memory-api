# [test-cli][log] test/log CLI + audit "failed & slow" query surface

## Goal

Give `test-api` and `log-api` a user-facing surface (CLI, optional HTTP) and an audit query that answers: "which validations failed, and which are unreasonably slow?".

## Scope

- A `test` CLI (under `memory-api/tools/cli/`) wrapping `test-api`: `record`, `get`, `list` (with `--outcome`, `--domain`, `--operation`, `--min-duration-ms`, `--slowest`, `--over-budget`), and a `summary` command rendering the store-index.
- A companion `log` surface (CLI or fold into `test`) to fetch `ValidationLogCapture` by execution id.
- An **audit query** ("board health"-style) that lists, from the test-api index: failed executions, over-budget benchmarks, and slow executions, ordered by severity.
- TOON + JSON output, consistent with other CLIs.
- Optional: a minimal HTTP surface mirroring the list/summary queries (defer if not needed).

## Acceptance criteria

- [ ] `test` CLI can record and query executions/benchmarks with outcome/domain/operation/duration filters and TOON/JSON output.
- [ ] A single audit command surfaces failed + over-budget + slow runs from the index.
- [ ] Log captures for an execution are retrievable via the CLI.
- [ ] Commands are documented with examples.

## Relationship / traceability

- Depends on the store-index generator (and the timing/benchmark models beneath it).
- Note: today `test-api` has only `test-mcp`; `log-api` has no surface — this closes that gap.
