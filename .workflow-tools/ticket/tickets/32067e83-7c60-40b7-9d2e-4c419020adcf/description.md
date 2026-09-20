## Implementation

Files touched:
- `memory-api/tools/mcp/mcp-cost-gate/src/gate.rs`
  - Added `Gate::resolves(&self, model: &str) -> bool` (thin wrapper over `resolve_output_mtok`).
  - Added `Gate::available_model_ids(&self) -> Vec<String>` (sorted, deduped `model_id`s).
  - `evaluate()` now passes `&self.available_model_ids()` into `unknown_model_guidance`.
  - `unknown_model_guidance(model, available)` signature changed (was `(model)`); text now
    documents the normalization tolerance and lists up to 12 detected available models.
- `memory-api/tools/mcp/mcp-cost-gate/src/proxy.rs`
  - Added `normalize_caller_model()`: strips a trailing parenthetical qualifier, folds
    spaces/underscores to hyphens, lowercases. No fuzzy/edit-distance matching.
  - `handle_client_message`: resolves the raw `caller_model` first (unchanged exact→substring
    precedence via `Gate::resolves`); only on failure retries with the normalized candidate.
    A normalized match still calls `gate.evaluate` (using the resolved model) and stays on the
    `Decision::Allow` path — call is forwarded, never rejected — but records a soft warning on
    the `PendingCall` and tags telemetry decision as `allow-normalized`.
  - `handle_server_message`: when the matching `PendingCall` carries a warning, injects a
    `costGateWarning` string field into the JSON-RPC `result` object of the eventual server
    response before returning it to the client.
  - `inject_caller_model_schema`: updated the injected `caller_model` argument description to
    mention the tolerated qualifier/separator fallback (this is the doc surface MCP clients see
    via `tools/list`).

## Normalization rule

Strip a trailing parenthetical qualifier (e.g. `"Claude Sonnet 5 (copilot)"` →
`"Claude Sonnet 5"`), then fold spaces/underscores to hyphens, then lowercase. Applied only in
`proxy.rs::handle_client_message`, only as a fallback after `Gate::resolves(raw)` returns false.

## Soft warning shape

`result.costGateWarning: string` added to the eventual JSON-RPC response for the matching
`tools/call` id when normalization was needed to resolve the model. No warning field is added
on an exact/substring match or on rejection.

## Tests added (all in `proxy.rs`, `mod tests`)

- `parenthetical_client_qualifier_is_tolerated` — `"gpt-5-mini (copilot)"` forwards (allow) and
  the eventual response carries `costGateWarning`; telemetry decision is `allow-normalized`.
- `space_and_underscore_separators_are_normalized` — `"Claude_Opus 4 1"` normalizes to
  `"claude-opus-4-1"` and forwards with a warning on response.
- `genuinely_unknown_model_still_rejected_after_normalization` — `"Totally Unknown Model
  (copilot)"` normalizes to something not in the price table and is still rejected.

## Validation

`rtk cargo test -p mcp-cost-gate` → `cargo test: 54 passed (4 suites, 0.05s)`.

## Spec

Spec `29ae5f6e-c202-41f1-ba88-a446aa872993` ("Empirical tool-metrics driven cost-gate
classification") documents tool-cost/budget classification, not `caller_model` string
resolution/normalization — it was not updated because this change doesn't alter the contract
it describes.

Re-applied the 53de6c5 fix that was clobbered by c58e9be eight minutes later. Restored in memory-api commit 70c9112 (proxy.rs: +144/-3), verified present at HEAD via `git grep normalize_caller_model HEAD`. cargo test -p mcp-cost-gate: 54 passed, 0 failed (4 suites). persistence.rs confirmed untouched. Parent repo submodule pointer bumped in commit 071acace; spec 9f0b9e30 updated with evidence_requirements/fulfillment_summaries recording the test result (commit 189e3dcc).