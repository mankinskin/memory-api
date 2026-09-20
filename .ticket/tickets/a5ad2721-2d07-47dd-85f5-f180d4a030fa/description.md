## Summary

Route every MCP tool call through a **model-aware wrapper** that enforces price-awareness policy at the tool boundary. Prompt templates, agent templates, and other guidance files already declare which MCP tools an agent/prompt may use, but that allow-list must be **conditioned on the size/cost of the calling model** so that large, expensive models do not accidentally invoke token-expensive tools.

Part of: [445a2d76 Model price awareness: enforce orchestrator mode for expensive models](../445a2d76-5795-4d7a-aec8-d1536ec61416/ticket.toml).

## Motivation

Guidance files bind tools to prompts/agents statically, but the same prompt can run on a cheap or an expensive model. Expensive models calling token-heavy MCP tools directly defeats the price-awareness goal. Enforcement therefore has to happen where the call is made — at the MCP tool boundary — not only via prose instructions.

## Approach

- Start **all** MCP tools through a common wrapper/proxy layer rather than exposing them directly.
- Require each caller to pass its **model identity** (and thereby its cost, resolved from the model→cost mapping in `model_prices.json`).
- The wrapper **filters/blocks** calls from models above the cost threshold `X` to token-expensive tools, returning a structured refusal that **explains the model must delegate** the call to a cheaper sub-agent instead.
- Cheap models pass through normally; expensive models are steered to orchestrator/delegation behavior.

## Scope / Deliverables

1. A wrapper/proxy entry point that fronts MCP tool invocation and requires a `model` (and/or cost) parameter.
2. Cost resolution from the model→cost mapping produced by [tools/model-prices/model_prices.json](../../../tools/model-prices/model_prices.json).
3. A per-tool classification of "token-expensive" vs. always-allowed tools (or a cost/impact tag per tool).
4. Enforcement logic: allow, or block-with-delegation-guidance, based on model cost vs. threshold `X`.
5. Integration so prompt/agent tool allow-lists are evaluated together with the model-cost condition.

## Acceptance Criteria

- MCP tools are reachable only through the wrapper.
- A call from a model above threshold `X` to a token-expensive tool is refused with a clear "delegate to a cheaper sub-agent" explanation.
- A call from a cheap model (or to an always-allowed tool) succeeds unchanged.
- Cost is resolved from the shared model→cost mapping, not hardcoded per tool.

## Update (2026-07-25)

**Threshold resolved (inherited from T-PARENT):** gate on `output_mtok > 15` USD per 1M output tokens (= 1500 credits/1M). Cost resolved from [tools/model-prices/model_prices.json](../../../tools/model-prices/model_prices.json).

**BLOCK-3 investigation (model-identity transport) — still needs a product decision.** There is currently **no caller-model channel at the MCP boundary**: every server under `memory-api/tools/mcp/*/src/server.rs` is a plain stdio JSON-RPC server that never receives the active model's identity. Three candidate transports:
- **A. Explicit tool parameter** (`caller_model` on each wrapped call). Simplest to build; weak enforcement because the model self-reports and could omit/spoof it.
- **B. Session/context header via the session store.** Record the active model on the durable workspace session (session-api) at session start; the wrapper reads it. Central, but requires the client to set it and a lookup per call.
- **C. Client/extension-layer interception.** Enforce in the VS Code extension where the active model IS known, before the call reaches any MCP server. Strongest (model can't spoof), but lives outside the Rust MCP crates.

Recommendation: **C for real enforcement, with A as a portable fallback** for non-VS-Code transports. Needs user confirmation before implementation.

## Open Questions

- ~~Which price field + threshold `X`~~ → resolved: `output_mtok > 15`.
- How the model identity is transported (A/B/C above) — **decision needed**.
- How each tool is classified as token-expensive (static tag vs. measured cost).