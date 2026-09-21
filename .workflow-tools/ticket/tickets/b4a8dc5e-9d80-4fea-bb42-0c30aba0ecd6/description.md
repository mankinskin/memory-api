# Rule rendering & instruction-surface redesign (remove always-on load)

Operationalizes decision **D7**: stop force-loading static guidance; make it discoverable and agent-rendered.

## Decision
Only a minimal **bootstrapper** instruction stays always-on. All other guidance becomes **discoverable** rule entries that an agent gathers (`rule_search`), pins, and renders into its own per-session instruction set. Rule *filters/scopes* may be pinned; individual rule bodies are fetched on demand (headers-only by default).

## Scope
- Author a minimal bootstrapper instruction (<500 tokens) that knows only the search + session tools and drives `session_init` → cascade → pin → agent-side render.
- Stop generating the always-on `applyTo: "**"` bodies as per-turn content; convert them into canonical, searchable-but-not-force-loaded rule entries. Target files:
  - `ticket-system.instructions.md` (447 lines)
  - `commit.instructions.md` (239 lines)
  - `spec-system.instructions.md` (180 lines)
  - `token-efficiency.instructions.md` (132 lines)
- Narrow remaining `applyTo` globs so only the bootstrapper is universal.
- **Fix the `spec-system.instructions.md` duplication** (`## Scope` at lines 6 and 94).
- Provide the agent-side render path: pinned rule entries/filters → focused session instruction set.
- Keep `rule sync-targets --check` deterministic for whatever remains generated.

## Depends on
- Design ticket (D1–D9 frozen) and the CLI/MCP pin/view surfaces (6b2dc497) so pulled-out guidance has a pin/render path.

## Risk to confirm
The always-on load is a VS Code Copilot `applyTo` mechanism; removing it means narrowing the generated globs and accepting that converted guidance is no longer guaranteed every turn — it is reachable only via bootstrap+pin.

## Spec
`memory-api/session-api/minimal-bootstrapper-selective-loading` (a28a88db).