## Problem

[unmeasured-tool-policy.md](.spec/specs/29ae5f6e-c202-41f1-ba88-a446aa872993/sections/unmeasured-tool-policy.md) assigns cost **0** to any tool absent from the rollup and always allows it: *"Unproven cost is never treated as high cost."*

The policy is correct — gating must be earned by evidence. But with **0%** of calls measured, its observable behaviour (every tool allowed, no error, no warning) was **identical** to a fully healthy, fully measured system. That is the single design decision that let a totally broken pipeline run invisibly for months while ten tickets shipped `done`. No "what fraction of tool calls are measured?" signal exists anywhere in the track.

User decision (2026-07-29): keep fail-open, add a mandatory coverage metric.

## Design

Add to `ToolMetricsReport` in [tool_metrics.rs](memory-api/crates/session-api/src/tool_metrics.rs):

- `measured_call_fraction: f64` — calls with a known `output_source` ÷ total calls in the window.
- `unmeasured_tools: Vec<String>` — tools with calls but no measured output, ordered by call count descending.

Surface both in the `session_tool_metrics` MCP tool, the `session tool-metrics` CLI output, and `tool-metrics-rollup.json`. When `measured_call_fraction` is below a configurable floor, the cost gate emits a visible warning naming the top unmeasured tools rather than silently costing them 0.

## Acceptance criteria

- AC1 — `tool-metrics-rollup.json` regenerated from the real store contains `measured_call_fraction` and `unmeasured_tools`, verified by reading the file. Record the observed fraction in the status summary.
- AC2 — With a rollup where no call has a measured output, the gate emits the low-coverage warning; with a fully measured rollup it does not. Both asserted by test.
- AC3 — Fail-open behaviour is unchanged: an unmeasured tool is still allowed. This ticket adds signal, not gating.
- AC4 — [unmeasured-tool-policy.md](.spec/specs/29ae5f6e-c202-41f1-ba88-a446aa872993/sections/unmeasured-tool-policy.md) is updated to state the coverage-metric obligation, so the policy can no longer be read as licence for zero measurement.