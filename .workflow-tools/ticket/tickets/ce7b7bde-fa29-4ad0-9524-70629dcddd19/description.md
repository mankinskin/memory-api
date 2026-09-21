## Problem

Three independent post-mortem agents (review, roast, session forensics) converged on one recurrence mechanism: **proxy evidence was accepted in place of outcome evidence**, repeatedly, for months.

Evidence:

- Ticket 84c7757d was marked `done` by **code-existence tracing** — "all 3 acceptance criteria traced to tool_metrics.rs, hook.rs, hook/tool_execution.rs" — while the file those criteria describe was `{"tools":{}}`.
- The validation record [.test/default/specs/val-session-api-lib-suite.json](.test/default/specs/val-session-api-lib-suite.json) is a per-module unit-test count ("148 tests passed: … tool_metrics::tests (10) …") with no artifact read-back. All ten of those tests hand-construct `role: Tool` turns the producer never emits.
- Session `d4868a37` correctly diagnosed "four independent breaks", created tickets 4aa13ba7 and 574560bf, then pivoted in the same session to unrelated review work. Both tickets sat `new` ever since while ~6 later sessions worked adjacent to them.

User decision (2026-07-29): add an enforced rule plus a mandatory e2e validation spec.

## Deliverables

1. **Instruction rule** under `.agents/instructions/` (testing scope): an acceptance criterion whose wording asserts data is *captured / collected / populated / persisted / recorded* must be verified by **reading the produced file or record**. Citing source code, a test name, or a passing-test count does not satisfy such an AC. The reviewer records the artifact path and the observed value.
2. **Validation spec** `val-session-api-tool-metrics-e2e` recorded in `.test`: drives a producer-shaped transcript through the real `copilot-capture-hook` binary and asserts the resulting `tool-metrics.json` has a non-empty `tools` map. Would have failed on every session prior to `7df14ea`.
3. **Corollary rule**: when a session's own diagnosis produces a new blocking ticket, that ticket becomes the next unit of work in the session unless the user explicitly defers it — directly targeting the `d4868a37` pivot.

## Acceptance criteria

- AC1 — The instruction file exists, is discoverable through the normal instruction-loading path, and names the forbidden evidence forms explicitly.
- AC2 — `val-session-api-tool-metrics-e2e` exists in `.test` with a recorded execution against the current code, verified by reading the execution record.
- AC3 — Deliberately reverting `compute_session_summary_with_events` to the turn-only computation makes the validation spec **fail**. This is the proof the gate is real; record the observed failure.
- AC4 — The corollary rule is added to the relevant orchestration instruction file and cross-references this ticket for the evidence.
## Completion evidence

**AC1** — New instruction file: [.agents/instructions/testing/data-capture-verification.instructions.md](.agents/instructions/testing/data-capture-verification.instructions.md). States that ACs asserting data is captured/collected/populated/persisted/recorded require artifact read-back; names forbidden proxy-evidence forms (code-existence tracing, test-name citation, passing-test counts) explicitly. Discoverable via the same frontmatter `description` convention as sibling files in `.agents/instructions/testing/`.

**AC2 + AC3** — New e2e test `e2e_val_session_api_tool_metrics_gate_asserts_nonempty_tools_map` in [memory-api/crates/session-api/tests/copilot_capture_hook_e2e.rs](memory-api/crates/session-api/tests/copilot_capture_hook_e2e.rs) (style of `e2e_hook_binary_populates_tool_metrics_from_captured_tool_events`): drives the real `copilot-capture-hook` binary over a producer-shaped transcript (tool.execution_start/complete events) and reads back `tool-metrics.json`, asserting the `tools` map is non-empty (`read_file.call_count == 1`).

- Validation spec recorded: `val-session-api-tool-metrics-e2e` (`.test/default/specs/val-session-api-tool-metrics-e2e.json`).
- Passing execution (current/restored code): `exec-val-session-api-tool-metrics-e2e-pass-2-restored` — outcome `passed`, `.test/default/executions/exec-val-session-api-tool-metrics-e2e-pass-2-restored.json`.
- Deliberate-failure execution (AC3 proof): `exec-val-session-api-tool-metrics-e2e-fail-turnonly-revert` — outcome `failed`, `.test/default/executions/exec-val-session-api-tool-metrics-e2e-fail-turnonly-revert.json`. Recorded after temporarily commenting out the event-processing loop in `compute_session_summary_with_events` (`memory-api/crates/session-api/src/tool_metrics.rs`), rerunning the test, and observing the panic (`tool-metrics.json` not found — with tools empty the hook lazily skips writing the artifact at all). Implementation was restored immediately after (confirmed via `git diff --stat` showing zero net change to `tool_metrics.rs`, and a full 6/6 passing rerun of the hook e2e suite).

**AC4** — Corollary rule added to [.agents/instructions/orchestration/loop-closure.instructions.md](.agents/instructions/orchestration/loop-closure.instructions.md) (Rule 5 + new Anti-Pattern entry): a session's own diagnosis that produces a new blocking ticket makes that ticket the next unit of work in the same session unless the user explicitly defers it. Cross-references this ticket (`ce7b7bde`) and the `d4868a37` pivot it targets.

All 4 ACs satisfied. Codebase confirmed not left in the reverted/broken state.