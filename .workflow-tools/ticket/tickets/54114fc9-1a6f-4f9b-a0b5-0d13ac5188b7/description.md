## Problem
Session/step sequencing today is agent-driven (LLMs decide control flow via MCP tool calls), which allows overstepping scope, indefinite iteration loops, and unpredictable termination. Ticket dependency data already exists in ticket-mcp/ticket-api but nothing consumes it deterministically to drive execution.

## Goal
Write a spec (not an implementation ticket yet, per explicit user decision) for a deterministic Rust-side state machine/controller that:
- Reads ticket dependencies from ticket-api to sequence steps instead of relying on an LLM to decide sequencing.
- Owns transitions between states such as Planning, Executing (with a step index), Testing (with a retry counter), Reviewing, Escalated.
- Terminates a worker sub-agent's session after its single step (see write-and-die ticket) and enforces retry-limit escalation (see retry-limit ticket) structurally rather than by instruction alone.
This is a foundational architecture change to how session-mcp/ticket-mcp control flow works between sessions, so it must be spec'd before any implementation ticket is opened.

## Acceptance criteria
- Spec defines the explicit state enum/transition graph, its inputs (ticket dependency data) and outputs (session dispatch calls).
- Spec explicitly scopes what stays LLM-driven vs. what becomes Rust-driven, and which existing crates/tools it would touch (ticket-api, session tooling).
- No implementation ticket is opened until this spec is reviewed and accepted.

## Source
Derived from AGENT_WORKFLOW_OPTIMIZATIONS.md conversation, "Step 2: The Rust Orchestration Engine". User decision: spec first, not a direct implementation ticket.