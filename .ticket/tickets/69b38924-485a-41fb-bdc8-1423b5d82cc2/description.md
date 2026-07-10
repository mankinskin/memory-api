Reduce redundant per-run work in sync_targets_payload without changing generated content.

Root causes (memory-api/tools/cli/rule-cli/src/cli/rendering.rs):
1. Double collection: `ensure_no_zero_match_targets` runs `collect_target_rules` over every target, then `generate_target_payload` collects the same rules again per target.
2. Unconditional writes: `write_generated_output` always writes even when prepared content byte-equals the existing file.
3. Per-target SpecStore reopen: `write_spec_generated_output`/`ensure_spec_generated_output_matches` call `open_spec_store_for_artifact` -> `SpecStore::open` + `scan(false)` for every spec-doc target.

Plan:
- Collect target rules once per target; feed the collected Vec into both zero-match validation and rendering (remove the separate full pre-pass, or have it reuse cached results).
- In write_generated_output, compare `prepared` to existing and skip `fs::write` when equal; surface a per-target `changed: bool` in the payload entry.
- Open one SpecStore per sync run (keyed by workspace root) and reuse it across spec-doc targets.

Acceptance (from spec 9c7c0655):
- AC2 skip-unchanged writes with `changed` flag in payload.
- AC3 single collection pass.
- AC4 SpecStore opened once per run.
- AC6 `--check` and pre-commit drift gate still correct.

Validation: cargo test -p rule-cli; cargo test -p rule-api; before/after timing of `rule sync-targets --config rule-targets.yaml`.