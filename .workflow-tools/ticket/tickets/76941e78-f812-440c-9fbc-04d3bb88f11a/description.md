## Problem

Tool output size is the one number the entire graded-cost design depends on (`graded-cost-scale.md`: "LINEAR map of empirical `est-output-tokens`"), and it has never been captured. The raw Copilot transcript's `tool.execution_complete` payload is only `{success, toolCallId}`.

Three candidate sources exist. **Nobody has ever established which of them actually carries the data.** This ticket is a bounded probe, not an implementation. It exists first because the whole prior track's failure mode was building consumers before proving the producer.

## Candidate sources to probe

1. **PostToolUse hook payload (highest value, unverified).** `.github/hooks/hooks.json` already runs `copilot-capture-hook --from-hook-stdin` on `PostToolUse`. Sibling scripts read `tool_input` from that same stdin — see [tools/agent-hooks/preflight-write.sh](tools/agent-hooks/preflight-write.sh) (`data.get('tool_input', {})`) and [tools/agent-hooks/validate-docs.sh](tools/agent-hooks/validate-docs.sh). Today [args.rs](memory-api/crates/session-api/src/bin/copilot-capture-hook/args.rs) `args_from_hook_stdin` extracts only `transcript_path`, `workspace_slug`, `hook_event_name` and **discards the entire rest of the payload**. If the payload carries `tool_response` / `tool_output`, we get exact output size for every tool with no new plumbing.
2. **Spill files (verified present).** `chat-session-resources/<session-id>/<tool_call_id>__vscode-<ts>/content.txt`. Confirmed 2026-07-29: the directory-name prefix before `__` **is** the `tool_call_id` from `events.json`, so byte counts are directly joinable. Covers spilled (large) results only — which are exactly the expensive ones. `has_spill` / `spill_pointer` already exist in [tool_execution.rs](memory-api/crates/session-api/src/hook/tool_execution.rs).
3. **MCP proxy telemetry (exists, switched off).** `mcp-cost-gate` computes `request_chars` / `response_chars`, but `COST_GATE_TELEMETRY_LOG` is unset in both [.vscode/mcp.json](.vscode/mcp.json) and [opencode.json](opencode.json), so records go nowhere. Tracked separately as 4aa13ba7. Covers MCP tools only.

## Acceptance criteria

- AC1 — A captured, checked-in sample of the **actual** `PostToolUse` hook stdin payload exists in the repo as a fixture, obtained by dumping real stdin to a file. Verified by reading the fixture, not by reasoning about the hook contract.
- AC2 — The probe states, per source, exactly which field yields output size, its units (bytes vs chars), and what fraction of this session's tool calls it covers. A source that turns out to carry nothing is recorded as such — a negative result closes this ticket successfully.
- AC3 — A written source-precedence recommendation for T2, ranked by coverage × fidelity.
- AC4 — No production code path is changed by this ticket.

## Non-goals

- Implementing capture. That is the child ticket.
## Probe results (2026-07-30)

### AC1 — Fixture captured

Real `PostToolUse` hook stdin dumped via a temporary one-line tee inserted into `tools/agent-hooks/terminal-pwd.sh` (already an active hook in this session), triggered by real tool calls, then fully reverted. Saved as:
- `memory-api/crates/session-api/tests/fixtures/posttooluse_hook_stdin_sample.json` — captured for a `replace_string_in_file` call.
- `memory-api/crates/session-api/tests/fixtures/posttooluse_hook_stdin_sample_terminal.json` — captured for a `run_in_terminal` call.

Real observed top-level payload keys: `timestamp, hook_event_name, session_id, transcript_path, tool_name, tool_input, tool_response, tool_use_id, cwd`. No `workspace_slug`/`workspaceSlug` key was ever observed in real stdin (contradicts the assumption embedded in `args_from_hook_stdin`'s field list — negative result, worth a follow-up note but out of scope here since AC4 forbids touching `args.rs`).

### AC2 — Per-source coverage/fidelity

| Source | Field | Unit | Coverage this session | Fidelity | Notes |
|---|---|---|---|---|---|
| 1. Hook payload | `tool_response` (top-level string) | JSON string chars | Populated for `run_in_terminal` non-spilled calls (verified: 28-char and ~190-char real samples). **Empty string `""`** for `replace_string_in_file` (verified real sample) — 0% for edit tools. For spilled (>~20KB) `run_in_terminal` calls, `tool_response` at hook-fire time already contains only the ~120-char pointer message ("Large tool result... written to file: <path>"), not the real output — the harness spills *before* the hook sees stdin. | High for non-spilled text-output tools (exact rendered chars); **zero direct signal** for edit tools; indirect-only for spilled calls (path must be joined to source 2). | Also observed: rapid consecutive tool calls can race — one hook invocation intermittently reads a stale/queued stdin buffer instead of the immediately-preceding call's payload. Any T2 consumer must correlate by `tool_use_id`, never by call order. |
| 2. Spill files | file size at `chat-session-resources/<session>/<tool_call_id>__vscode-<ts>/content.txt` | **bytes** (`wc -c` confirmed exact: 20000 bytes matching the harness's own "20KB" message) | Only tool calls whose output exceeds the spill threshold — 2 of the tool calls actually made in this probe session. Directory-prefix-before-`__` == `tool_call_id` reconfirmed live (matches ticket's prior claim). | Byte-exact, the best fidelity of the three sources, but narrowest coverage (large/expensive calls only — which are also the ones that matter most for cost). | `has_spill`/`spill_pointer` in `tool_execution.rs` already exist to carry this join key; unchanged by this probe. |
| 3. MCP proxy telemetry (`mcp-cost-gate`) | `request_chars`/`response_chars` (per prior track's design) | chars | **0%** — reconfirmed negative: `COST_GATE_TELEMETRY_LOG` is absent from both `.vscode/mcp.json` and `opencode.json` (grep, no matches). | N/A — telemetry never emitted, nothing to measure. | Negative result stands; tracked separately as ticket `4aa13ba7`. |

### AC3 — T2 source-precedence recommendation (ranked by coverage × fidelity)

1. **Spill-file byte stat (source 2) as the primary signal for large/expensive calls.** Byte-exact, and these are precisely the tool calls that matter most for the graded-cost design. Join key (`tool_call_id` == directory prefix before `__`) is already confirmed and `has_spill`/`spill_pointer` already exist in `tool_execution.rs` — lowest-risk integration point.
2. **Hook-payload `tool_response` char length (source 1) as the fallback for non-spilled calls.** Covers the remainder of `run_in_terminal`-style tool calls that never spill, at char (not byte) fidelity. Must NOT be trusted at face value for spilled calls (it will read as the short pointer message, wildly undercounting) — T2 must special-case: if `tool_response` matches the "Large tool result... written to file" pointer pattern, fall back to source 2 instead of using its literal length. Also do not assume it is present for all tool kinds — it was empty for `replace_string_in_file`; needs a per-tool-kind allowlist rather than a blanket read.
3. **MCP telemetry (source 3): do not build against it yet.** It is fully dark (env var unset in every config). Do not implement a T2 consumer path for it until `4aa13ba7` turns the env var on and a real emitted record can be captured the same way this probe captured source 1 — building the consumer first was the prior track's exact failure mode this ticket exists to prevent.
4. Cross-source correlation must key off `tool_use_id` (hook payload) / `tool_call_id` (spill dir, transcript events), not arrival order — the hook-payload race observed above means order-based joins will silently corrupt data.

### AC4 — No production code path changed

Confirmed via `git status`/`git diff`: only new files are the two fixtures above under `tests/fixtures/`. The temporary tee line added to `tools/agent-hooks/terminal-pwd.sh` and a temporary hooks.json entry were both fully reverted — `git status --short` shows zero diff on `tools/agent-hooks/terminal-pwd.sh` and `.github/hooks/hooks.json`. `args.rs`, `tool_execution.rs`, and `tool_metrics.rs` were not touched. No `cargo test` run required (no session-api src changed); an unrelated pre-existing modification to `crates/session-api/tests/copilot_capture_hook_e2e.rs` from a different, in-progress ticket (`ce7b7bde`) was observed and left untouched.

All 4 ACs satisfied. Moving to `in-review`.