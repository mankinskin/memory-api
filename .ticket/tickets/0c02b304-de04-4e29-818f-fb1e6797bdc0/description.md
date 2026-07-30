## Resolution

### Defect 1 — history append failures silently swallowed
- `TicketFs::read_history` (memory-api/crates/ticket-api/src/storage/ticket_fs.rs) now skips a malformed NDJSON line instead of erroring, logging `tracing::warn!` with the line number and parse error. Since `append_history` calls `read_history` to compute the next rev number, a single corrupt line previously wedged every future append forever; now it is quarantined (skipped) and reported.
- All 7 audited `let _ = TicketFs::append_history(...)` sites in ticket-api now wrap the result in `if let Err(error) = ... { tracing::error!(...) }`:
  - `memory-api/crates/ticket-api/src/storage/store.rs` (`create` initial history snapshot, and `update_ticket`)
  - `memory-api/crates/ticket-api/src/storage/store/lifecycle.rs` (`attach` asset history)
  - `memory-api/crates/ticket-api/src/storage/store/parts.rs` (`write_part`, `write_amendment_part`, `undo_part`)
  - `memory-api/crates/ticket-api/src/storage/store/query.rs` (`add_edge`, `remove_edge`)
- Pattern per site is **log loudly (`tracing::error!`) and continue**: by the time history is appended, the manifest/part/edge/attachment write has already committed to disk and is the system of record. Propagating the error would report failure for a mutation that actually succeeded, risking a caller retry that double-applies the patch. The history file is a best-effort undo trail — losing an entry must be visible, not silent, but must not mask a successful write as a failure.
- Regression tests added in `ticket_fs/tests.rs`: `read_history_skips_malformed_line_instead_of_erroring`, `append_history_still_works_after_a_malformed_line` (reproduces the exact `[]` corruption found in production).
- Regression coverage for the 7 ticket-api call sites added in `storage/tests/history_append_failure_tests.rs`: `create_logs_error_when_initial_history_append_fails`, `write_part_logs_error_when_history_append_fails`, `write_amendment_part_logs_error_when_history_append_fails`, `undo_part_logs_error_when_history_append_fails`, `attach_logs_error_when_history_append_fails`, `add_edge_logs_error_when_history_append_fails`, `remove_edge_logs_error_when_history_append_fails`. Each test forces the history file read-only and asserts the operation still returns `Ok` while the swallowed error is logged.

### Defect 2 — manifest null becomes ""
- `write_toml_kv`'s `Value::Null` branch (memory-api/crates/memory-api/src/model/manifest_format.rs) now omits the key entirely instead of writing `key = ""`.
- `TicketFs::update` (ticket_fs.rs) now treats a `Value::Null` patch entry as "remove this key" (`manifest.extra.remove(k)`) rather than inserting it.
- **Caller audit**: grepped all `field_map`/`fields` construction sites (rule-mcp, spec-mcp, ticket-mcp `parse_field_patch`/`parse_fields`). `field_map` is always `Option<BTreeMap<String, Value>>` taken directly from the raw incoming JSON-RPC request body — no internal Rust caller builds a patch by serializing a struct with `Option<T>` fields (no `serde_json::to_value` on an Option-bearing type feeds into a patch anywhere in ticket-api/ticket-mcp/spec-mcp/rule-mcp). A `null` can only appear if a caller explicitly writes `"field_map": {"key": null}` in the request — that is the deliberate deletion signal being added, not an accidental artifact. No sentinel/gating needed; risk assessed as not real given current call sites.
- Regression tests: `manifest_format/tests.rs` (`null_value_is_omitted_from_serialized_toml`, `explicit_empty_string_is_preserved_not_treated_as_deletion`), `ticket_fs/tests.rs` (`update_with_null_patch_value_removes_the_key`, `update_with_explicit_empty_string_is_not_treated_as_deletion`).

## Validation
- `cargo build --workspace`: 0 errors.
- `cargo test -p ticket-api --lib`: all ticket-api lib tests pass.
- Store scan `.ticket/` (root): diagnostics `[]`, 825 valid ticket manifests.
- Store scan `memory-api/.ticket/`: diagnostics `[]`, 329 valid ticket manifests.
- Note: task text cited baseline counts of 1356/331; actual current counts are 825/329. Counts were not affected by this change (no tickets touched/deleted); the discrepancy predates this work and is likely drift since the task was authored.

## Not satisfied
- The 7 audited discarded-`Result` `append_history` sites **outside** ticket-api remain unfixed in this ticket per the user's scope decision. They are in rule-api (`src/store.rs` L475, L588, L679 and `src/store/generated_targets.rs` L129, L192) and spec-api (`src/store.rs` L428, L683) and are tracked in a follow-up ticket. No freeze/description_mode/part-addressing/state-vocabulary changes were made.
