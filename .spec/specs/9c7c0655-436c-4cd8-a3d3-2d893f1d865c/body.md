<!-- aligned-structure:v1 -->

# Summary

`rule sync-targets` is slow and emits inconsistent path separators. This spec
defines two contract changes: (1) sync must only perform work for rules/targets
whose inputs or outputs actually changed, and (2) every path field in the
command output must use forward-slash separators regardless of host OS.

# Problem / Current State

## Inefficiency (redundant work every run)

Three layers each re-do full work on every invocation, independent of whether
any rule changed:

1. Store scan is non-incremental. `EntityStore::scan_once` walks every rule
   folder, reads+parses every manifest, and calls `integrate_entry` which
   inserts every entity into the metadata index unconditionally — no mtime or
   content-hash short-circuit.
   - Evidence: `memory-api/crates/memory-api/src/storage/entity_store.rs` scan_once + integrate_entry.
2. Target rules are collected twice. `sync_targets_payload` runs
   `ensure_no_zero_match_targets` (a full `collect_target_rules` pass over all
   targets) and then `generate_target_payload` collects the same rules again
   per target.
   - Evidence: `memory-api/tools/cli/rule-cli/src/cli/rendering.rs` sync_targets_payload / ensure_no_zero_match_targets / generate_target_payload.
3. Outputs are re-written unconditionally. `write_generated_output` always
   writes even when the prepared content equals the on-disk content, and each
   spec-doc target reopens and re-scans a fresh `SpecStore`
   (`open_spec_store_for_artifact` -> `SpecStore::open` + `scan`) per target.

## Mixed path separators

The JSON/TOON payload mixes separators within a single run:
- `generated[].output` and top-level `config` are emitted as raw `PathBuf`,
  which serializes with OS separators (backslashes on Windows).
- `removed[].output` uses `stable_output_key`, which normalizes to `/`.
- Evidence: `memory-api/tools/cli/rule-cli/src/cli/rendering.rs`
  sync_target_payload_entry (`"output": output`) vs `stable_output_key`.

# Scope

- `rule sync-targets` (and shared paths in `generate-target`, MCP `generate`)
  command output and write behavior.
- Store-level incremental scan for the rule entity store.

# Non-Goals

- Changing the rule authoring format or generated-file markup.
- Changing which targets exist or their rendered content.
- A filesystem watcher / daemon mode.

# Acceptance Criteria

1. Path normalization: every path field in `sync-targets` output (`config`,
   `generated[].output`, `removed[].output`) uses `/` separators on all hosts.
   A unit test asserts no `\\` appears in any emitted path on Windows-style
   inputs. `generate-target` and MCP `generate` payloads are consistent.
2. Skip-unchanged writes: when a target's prepared output byte-equals the
   existing file, sync does not rewrite the file. Payload reports a per-target
   `changed: bool` (or equivalent) so callers can see what was actually written.
3. Single collection pass: target rules are collected at most once per target
   per run (zero-match validation reuses the collected result).
4. Spec-store reuse: a single `SpecStore` is opened once per sync run and reused
   across all spec-doc targets instead of reopening per target.
5. Incremental store scan: re-running sync with no changed rule files performs
   no per-entity metadata re-integration for unchanged entities (verified by a
   test that counts integrate/index writes, or by an mtime/hash guard).
6. `sync-targets --check` behavior and the pre-commit drift gate remain
   correct: unchanged inputs pass, drift still fails.

# Traceability / Evidence

- Tickets: linked in this spec's ticket set (tracker + implementation tickets).
- Validation: `cargo test -p rule-cli`, `cargo test -p rule-api`,
  `cargo test -p memory-api`; plus a timing before/after of
  `rule sync-targets --config rule-targets.yaml` and a `--check` no-op run.
