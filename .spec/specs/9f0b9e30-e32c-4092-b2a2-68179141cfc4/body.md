<!-- aligned-structure:v2 -->

## Motivation
`mcp-cost-gate` must tolerate the common caller_model naming deviations that appear at the MCP transport boundary without weakening price-awareness enforcement. The contract is intentionally narrow: raw resolution still decides first, normalization is only a fallback, and a normalized success is treated as a compatibility recovery, not as proof that a model string is generally valid.

This is a distinct contract slice from the existing tool-cost / budget classification spec `29ae5f6e-c202-41f1-ba88-a446aa872993`; that spec remains untouched.

## Dependent expectation
If this spec is implemented, dependents can rely on the following behavior:
- Exact model_id matching still wins over substring matching.
- Normalization never runs before raw exact/substring resolution has failed.
- The transport wrapper, not the gate, owns caller_model normalization fallback.
- Normalization strips one trailing parenthetical client qualifier and folds spaces and underscores to hyphens.
- A normalized successful resolution is allowed, emits a soft warning as `result.costGateWarning`, and records telemetry decision `allow-normalized`.
- Genuinely unknown model strings remain rejected; normalization never converts an unknown model into a permitted zero-cost call.
- Rejection guidance includes only a bounded sample of available model ids and must never dump the full table.

## Guards
Validation for this contract is anchored by:
- `rtk cargo test -p mcp-cost-gate`
- `parenthetical_client_qualifier_is_tolerated`
- `space_and_underscore_separators_are_normalized`
- `genuinely_unknown_model_still_rejected_after_normalization`

## Positions
- `memory-api/tools/mcp/mcp-cost-gate/src/gate.rs` — implemented: `resolve_output_mtok` keeps exact-before-substring precedence, `evaluate` rejects unresolvable caller_model values before budget math, and `unknown_model_guidance` caps the displayed model sample.
- `memory-api/tools/mcp/mcp-cost-gate/src/proxy.rs` — implemented: `normalize_caller_model`, `handle_client_message`, `handle_server_message`, and `inject_caller_model_schema` together implement fallback-only normalization, soft-warning surfacing, and client-facing schema documentation.
- `memory-api/tools/mcp/mcp-cost-gate/src/proxy.rs` tests — implemented: the named tests cover parenthetical suffix tolerance, separator normalization, and rejection after normalization.

## Governing-rule requirement
This spec is currently governed by the general `memory-api/rule-api/rule-introduces-spec` policy until a dedicated `mcp-cost-gate` PolicyRule exists. If a narrower PolicyRule is added later, it should supersede this provisional coverage without changing the contract stated here.
