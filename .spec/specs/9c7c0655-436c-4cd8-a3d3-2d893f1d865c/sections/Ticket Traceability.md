Tracker:
- 6e7f9bd6 [rule-cli] sync-targets: incremental work + normalized path output
  (.ticket/tickets/6e7f9bd6-80e8-4f0d-86d9-128eb257eb32/ticket.toml)

Implementation (children):
- 2dc02b9b Normalize path separators in sync-targets output
  (.ticket/tickets/2dc02b9b-7e94-4cb4-82c4-83e4359792cb/ticket.toml) -> AC1 [done]
- 69b38924 skip unchanged writes, single collection pass, reuse SpecStore
  (.ticket/tickets/69b38924-485a-41fb-bdc8-1423b5d82cc2/ticket.toml) -> AC2, AC3, AC4, AC6 [done]
- 45f5f58e Incremental entity-store scan (mtime/hash short-circuit)
  (.ticket/tickets/45f5f58e-4a6e-40b9-bf03-1bc9dc5ca43d/ticket.toml) -> AC5 [done]

Validation evidence (captured):
- AC1: `cargo test -p rule-cli` green incl. new
  `sync_targets_emits_forward_slash_path_fields`; real-config dry-run
  emits config + generated[].output with `/` (no `\`). rule-mcp generate.rs
  output_path fields normalized via `display_path`. rule-mcp builds clean.
- AC2/AC3/AC4/AC6: `cargo test -p rule-cli` green incl. new
  `sync_targets_reports_changed_flag_and_skips_unchanged_writes` (verifies
  `changed:true` on first write, `changed:false` + unchanged mtime on repeat).
  Real config: apply on synced tree reports `changed 0 of 86`; `--check`
  passes clean (drift gate intact). `cargo test -p rule-api` green (78).
  Single rule-collection pass + one SpecStore per workspace-root per run.
- AC5: `cargo test -p memory-api` green (136) incl. new
  `scan_skips_unchanged_entities_across_consecutive_scans` (scan(true) then
  scan(false) => integrated 0; touch one manifest => integrated 1). Shared-
  store crates green: ticket-api 107 pass (1 pre-existing unrelated failure
  `preflight_reports_invisible_reference_visibility_and_path_refs`, fails
  identically on baseline without this change), spec-api 55, log-api,
  audit-api. Fingerprint sidecar `scan_fingerprints.json` written per
  index-root and git-ignored.

Timing (real `rule sync-targets --config rule-targets.yaml`, debug build):
- before change: ~1m40s per run (full re-integration every run).
- after change: prime run ~1m20s, incremental no-change run ~1m0s.
- The remaining fixed cost is per-store scan/search-consistency overhead
  outside AC5 scope; AC5 (no per-entity re-integration for unchanged
  entities) is proven by the unit test and the fingerprint-skip delta.
