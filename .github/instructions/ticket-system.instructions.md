<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=588a853a-e235-439b-82b2-e9b16a6afbac slug=shared/instructions/ticket-system/ticket-system-instructions/l1 -->
---
description: "Use when editing ticket system crates/tools. Covers state transitions, storage/search boundaries, and workflow expectations."
applyTo: "**"
---

<!-- rule-api:entry id=ab548b37-b0c6-4ce0-bcc3-4126794ff4a8 slug=shared/instructions/ticket-system/ticket-system-guidance/scope/l8 -->
## Scope

Applies to:
- `crates/ticket-api/`
- `tools/ticket-cli/`
- `tools/ticket-http/`
- `tools/ticket-mcp/`
- `tools/ticket-viewer/`

<!-- rule-api:entry id=8d50ed12-70e5-4dc1-929e-389aa6b2ee8e slug=shared/instructions/ticket-system/ticket-system-guidance/design-constraints/l17 -->
## Design Constraints

- Respect ticket lifecycle/state machine invariants.
- Keep storage/index behavior backward compatible unless explicitly requested.
- Preserve clear separation between API, storage, transport, and UI layers.

<!-- rule-api:entry id=d19a35e2-eeb9-43b5-8fd6-341194801b31 slug=shared/instructions/ticket-system/ticket-system-guidance/ticket-quality-standing-obligations/l23 -->
## Ticket Quality — Standing Obligations

These rules apply during **every session**, not only when working on ticket-system code.

<!-- rule-api:entry id=296f828a-212e-4a0a-b326-145d2a47fd99 slug=shared/instructions/ticket-system/ticket-system-guidance/ticket-quality-standing-obligations/orientation-start-of-every-session/l27 -->
### Orientation (start of every session)

Before writing any code, run a quick orientation to understand the current ticket landscape:

<!-- rule-api:entry id=ff999de8-3c2f-46fd-886f-1b238fd4f975 slug=shared/instructions/ticket-system/check-the-draftboard-active-agents-wip-limit-stale-warnings/l44 -->
# Check the draftboard (active agents, WIP limit, stale warnings)
./target/debug/ticket.exe board show --json
```

<!-- rule-api:entry id=c00fd519-ed78-4421-867b-30cf9709e429 slug=shared/instructions/ticket-system/check-the-draftboard-active-agents-wip-limit-stale-warnings/l48 -->
Alternatively, use the MCP ticket tools (`mcp_ticket-mcp_next_tickets`, `mcp_ticket-mcp_list_tickets`, `mcp_ticket-mcp_health`, `mcp_ticket-mcp_board_show`) when the MCP server is running.

<!-- rule-api:entry id=70f9b9ce-7dff-4b81-aeaa-5ba71e029a8f slug=shared/instructions/ticket-system/check-the-draftboard-active-agents-wip-limit-stale-warnings/discovery-before-creating/l50 -->
### Discovery Before Creating

Always search for existing tickets before creating new ones. Duplicate tickets degrade store quality.

<!-- rule-api:entry id=ad1bbb84-8de6-468f-b031-6e08aa4b935d slug=shared/instructions/ticket-system/check-the-draftboard-active-agents-wip-limit-stale-warnings/discovery-before-creating/l54 -->
```bash
./target/debug/ticket.exe search "<keywords>" --json
```

<!-- rule-api:entry id=e5f54ef3-e247-4434-a2ef-27b5d59dd656 slug=shared/instructions/ticket-system/check-the-draftboard-active-agents-wip-limit-stale-warnings/discovery-before-creating/l58 -->
Or via MCP: `mcp_ticket-mcp_list_tickets` with a `where` filter, or `mcp_ticket-mcp_get_ticket_description`.

<!-- rule-api:entry id=536ca40a-cda8-46f7-b93f-e9d9dcdc0adb slug=shared/instructions/ticket-system/check-the-draftboard-active-agents-wip-limit-stale-warnings/discovery-before-creating/continuous-state-updates/l60 -->
### Continuous State Updates

Update ticket state immediately when the work status changes — do not defer to the end of a session:

<!-- rule-api:entry id=7b85403c-efe0-4a2d-9d5a-a6a4c371e67e slug=shared/instructions/ticket-system/check-the-draftboard-active-agents-wip-limit-stale-warnings/discovery-before-creating/continuous-state-updates/l64 -->
| Situation | Action |
|---|---|
| Starting implementation | `update --state in-implementation` |
| Implementation complete, moving to review | `update --state in-review` |
| All acceptance criteria met and validated | `close <id>` |
| Ticket is no longer relevant | `cancel <id>` with a reason |

<!-- rule-api:entry id=bee116c8-908b-4ecd-b753-88d67f37687b slug=shared/instructions/ticket-system/check-the-draftboard-active-agents-wip-limit-stale-warnings/discovery-before-creating/continuous-state-updates/l71 -->
> **IMPORTANT — state machine is one-way.** Transitions only go forward.
> Progress through **every** state in order — do not skip states. The schema
> defines `required_states` (e.g. `["in-review"]`) that **must** appear in a
> ticket's history before it can reach a terminal state (`done`). Attempting to
> close a ticket without visiting all required states will be rejected by the
> store.
>
> If a state was reached prematurely, use `update --undo` to revert the last
> transition and re-progress correctly.

<!-- rule-api:entry id=e53c1b38-e5b2-4f09-9f5f-124ad16f3854 slug=shared/instructions/ticket-system/check-the-draftboard-active-agents-wip-limit-stale-warnings/discovery-before-creating/correcting-state-transitions-undo-revert/l81 -->
### Correcting State Transitions (Undo / Revert)

If a ticket was advanced to the wrong state, use `--undo` to roll back the
last transition:

<!-- rule-api:entry id=b764e251-8216-458f-846b-a4ecdd3a72ad slug=shared/instructions/ticket-system/check-for-stale-in-implementation-tickets-that-may-conflict-with-your-work/l38 -->
# Check for stale in-implementation tickets that may conflict with your work
./target/debug/ticket.exe list --where state=in-implementation --json

<!-- rule-api:entry id=384ad946-63e0-4b0e-9e6d-ce721eb41491 slug=shared/instructions/ticket-system/survey-all-new-tickets/l35 -->
# Survey all new tickets
./target/debug/ticket.exe list --where state=new --json

<!-- rule-api:entry id=cb58d6b7-5318-47ae-a918-1636c88adb6f slug=shared/instructions/ticket-system/check-overall-graph-health/l41 -->
# Check overall graph health
./target/debug/ticket.exe health --all --json

<!-- rule-api:entry id=65d1ccdd-fe32-4d3b-aa31-47f68e48a637 slug=shared/instructions/ticket-system/undo-the-most-recent-state-change-reverts-to-the-previous-state/l86 -->
```bash
# Undo the most recent state change (reverts to the previous state)
./target/debug/ticket.exe update <id> --undo --json
```

<!-- rule-api:entry id=093d8024-c270-4463-9c77-a7d815064380 slug=shared/instructions/ticket-system/undo-the-most-recent-state-change-reverts-to-the-previous-state/l91 -->
For deeper rollbacks, use `revert --to <rev>` to restore a specific historical
revision:

<!-- rule-api:entry id=3a8b8069-5433-4370-aa19-795a30c10ace slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/l94 -->
```bash
# Revert to revision 6 (re-applies fields from that point in history)
./target/debug/ticket.exe revert <id> --to 6 --json
```

<!-- rule-api:entry id=5fe4b995-952f-475a-8a95-4a84424861d1 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/l99 -->
> `--undo` is a convenience for the common case of "I advanced too far" and is
> equivalent to reverting to rev N-2. Neither command deletes history — a new
> revision is appended recording the rollback.

<!-- rule-api:entry id=9340dab1-1948-4bff-9d8c-5d9aab6349a2 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/l103 -->
### Schema-Enforced Workflow (`required_states`)

The ticket type schema can declare `required_states` — a list of states that
**must** appear in a ticket's history before the store allows a transition to
a terminal state (default terminal: `done`).

<!-- rule-api:entry id=96d40c37-662a-4514-81db-0b0aeb8023b1 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/l109 -->
For `tracker-improvement` tickets, the schema enforces:

<!-- rule-api:entry id=c050a4a6-26e3-48bc-a524-29f82fcd4a92 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/l111 -->
```toml
required_states = ["in-review"]
```

<!-- rule-api:entry id=42719b2d-ecad-4a89-b7a1-d846106460b4 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/l115 -->
This means the store will **reject** `close` (or `update --to-state done`) if
`in-review` has never been visited. This is enforced at the API layer, so it
applies to CLI, MCP, and HTTP equally.

<!-- rule-api:entry id=dec367bc-2fd7-4c40-bf75-e7218cf4b060 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/l119 -->
To customize enforcement per ticket type, edit the corresponding schema file
under `crates/ticket-api/schemas/<type>.toml`.

<!-- rule-api:entry id=58268aab-1ae5-48c4-bf3e-e8b5e4e31fc0 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/review-gate-before-closing/l122 -->
### Review Gate Before Closing

**Never `close` a ticket directly from `in-implementation`.** Always move
through `in-review` first, even for small changes.
The schema's `required_states` enforcement prevents skipping `in-review`,
but you should still follow the full progression diligently.

<!-- rule-api:entry id=7b25ce1e-e49e-45b4-a938-26fef4e07035 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/review-gate-before-closing/step-1-move-to-in-review/l129 -->
#### Step 1 — Move to in-review

```bash
./target/debug/ticket.exe update <id> --state in-review
```

<!-- rule-api:entry id=5c7967bc-d3eb-40ab-a424-ebf6f6ff71b5 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/review-gate-before-closing/step-2-code-review-checklist/l135 -->
#### Step 2 — Code Review Checklist

Before moving to validation, verify each of the following. Fix any issue
found before proceeding:

<!-- rule-api:entry id=ce774188-d116-46fc-a5ab-aeec2081b386 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/review-gate-before-closing/step-2-code-review-checklist/l140 -->
**Correctness & Reactivity (frontend)**
- [ ] All signal reads that must re-run on change are inside reactive closures,
      not computed once outside the `view!` macro.
- [ ] State updated correctly on all paths (including edge cases like empty data).

<!-- rule-api:entry id=2b535b58-c7de-423f-bb02-eb663ea642c5 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/review-gate-before-closing/step-2-code-review-checklist/l145 -->
**Memory & Cleanup**
- [ ] No unbounded `Closure::forget()` calls; use `Closure::into_js_value()` to
      transfer ownership to the JS GC instead.
- [ ] Document-level event listeners registered with a `on_cleanup` removal hook
      so they are unregistered if the component unmounts mid-gesture.
- [ ] No `Rc`/`RefCell` or wasm-bindgen closures that outlive component scope
      without an explicit cleanup path.

<!-- rule-api:entry id=ae8341ae-bab9-4804-a0ee-21b39d34d33e slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/review-gate-before-closing/step-2-code-review-checklist/l153 -->
**CSS & Layout**
- [ ] Elements with negative positioning checked against any `overflow: hidden`
      ancestors — they will be clipped.
- [ ] Responsive/resize behavior tested at both min-width and large widths.
- [ ] `aria-label` or role attributes on interactive elements without visible text.

<!-- rule-api:entry id=e4e48e92-b01a-4907-a335-bc68faf895d3 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/review-gate-before-closing/step-2-code-review-checklist/l159 -->
**Security**
- [ ] User-controlled strings inserted into the DOM use text-node APIs (e.g.
      `set_text_content`) — not `set_inner_html` — to prevent XSS.
- [ ] URLs derived from external data are validated before fetch/navigation.

<!-- rule-api:entry id=08a5736f-ced2-4676-a983-3f665f7eeb66 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/review-gate-before-closing/step-2-code-review-checklist/l164 -->
**General**
- [ ] No dead code, unused imports, or unreachable branches left behind.
- [ ] Public API changes reflected in docs/changelogs if applicable.

<!-- rule-api:entry id=06f4c9f7-64b8-4652-a198-5a970057cb84 slug=shared/instructions/ticket-system/revert-to-revision-6-re-applies-fields-from-that-point-in-history/schema-enforced-workflow-required-states/review-gate-before-closing/step-3-validate-acceptance-criteria/l168 -->
#### Step 3 — Validate Acceptance Criteria

Run the relevant test suite(s) against the ticket's acceptance criteria:

<!-- rule-api:entry id=c3672701-5962-4444-a900-baefee26b9dd slug=shared/instructions/ticket-system/native-unit-tests-pure-rust-logic-no-browser-needed/l172 -->
```bash
# Native unit tests (pure-Rust logic, no browser needed)
cargo test -p <crate>

<!-- rule-api:entry id=01c0e319-6cc9-46ea-b891-7cb9a37ca328 slug=shared/instructions/ticket-system/wasm-browser-tests-requires-wasm-pack-chrome/l176 -->
# WASM browser tests (requires wasm-pack + Chrome)
wasm-pack test --headless --chrome <path/to/crate>

<!-- rule-api:entry id=37587f7f-e83c-4132-9051-440361968be5 slug=shared/instructions/ticket-system/cargo-check-for-wasm-target-quick-compile-gate/l179 -->
# Cargo check for WASM target (quick compile gate)
cargo check --target wasm32-unknown-unknown -p <crate>
```

<!-- rule-api:entry id=eb6d742e-6022-4ced-b2c2-62f3ddbe3958 slug=shared/instructions/ticket-system/cargo-check-for-wasm-target-quick-compile-gate/l183 -->
Confirm each acceptance criterion listed in the ticket description is met with
a passing test or a documented manual verification step.

<!-- rule-api:entry id=d3bc2a47-9528-4399-84fa-c2411cd0264d slug=shared/instructions/ticket-system/cargo-check-for-wasm-target-quick-compile-gate/step-3-close/l186 -->
#### Step 3 — Close

Only close after the review checklist is complete and tests pass:

<!-- rule-api:entry id=ad74a898-7f23-4408-beb8-ffd5b290df60 slug=shared/instructions/ticket-system/cargo-check-for-wasm-target-quick-compile-gate/step-3-close/l190 -->
```bash
./target/debug/ticket.exe close <id>
```

<!-- rule-api:entry id=9b93d1d2-1c3d-4b48-a8ac-e72b73aa616c slug=shared/instructions/ticket-system/cargo-check-for-wasm-target-quick-compile-gate/step-3-close/picking-next-work/l194 -->
### Picking Next Work

Use `ticket next` to find the highest-priority unblocked tickets:

<!-- rule-api:entry id=efbe3dcd-4b64-4ff0-a6ea-19eb84fcf70c slug=shared/instructions/ticket-system/find-unblocked-ready-tickets-you-can-work-on-now-priority-ordered/l31 -->
```bash
# Find unblocked ready tickets you can work on now (priority-ordered)
./target/debug/ticket.exe next --json

<!-- rule-api:entry id=11d31eeb-c6ad-44a3-9fba-b7b0566642c8 slug=shared/instructions/ticket-system/list-unblocked-tickets-sorted-by-progress-then-priority/l198 -->
```bash
# List unblocked tickets sorted by progress, then priority
./target/debug/ticket.exe next --json

<!-- rule-api:entry id=a575836a-1397-4cd6-9c7d-c3315cba7c0b slug=shared/instructions/ticket-system/with-a-title-prefix-filter-for-a-specific-track/l202 -->
# With a title prefix filter for a specific track
./target/debug/ticket.exe next --filter "[bootstrap]" --json

<!-- rule-api:entry id=994b427a-6c92-4501-8804-6b30340cdd60 slug=shared/instructions/ticket-system/limit-results/l205 -->
# Limit results
./target/debug/ticket.exe next --limit 5 --json
```

<!-- rule-api:entry id=642e5f19-8a2d-44a7-ad8d-0ba34076e84e slug=shared/instructions/ticket-system/limit-results/l209 -->
Or via MCP: `mcp_ticket-mcp_next_tickets` with `workspace`, optional `limit` and `filter`.

<!-- rule-api:entry id=7da22b3b-7f84-4e4e-a366-131312a86388 slug=shared/instructions/ticket-system/limit-results/l211 -->
The command returns tickets in **any non-terminal state** whose `depends_on`
edges all point to `done`/`cancelled` tickets. Results are sorted by:

<!-- rule-api:entry id=a9299a3f-6491-4252-90d4-51873ba03ded slug=shared/instructions/ticket-system/limit-results/l214 -->
1. **State progress** — tickets closest to `done` appear first (e.g.
   `in-review` > `in-implementation` > `ready` > `new`).
   Progress is determined by the state's index in the schema's `states` list.
2. **Priority** — `critical > high > medium > low > none`.
3. **Creation date** — oldest first (FIFO tiebreaker).

<!-- rule-api:entry id=89436237-4491-446e-b43e-da6c1444b7b2 slug=shared/instructions/ticket-system/limit-results/l220 -->
**Dependency direction convention:** Parents/epics `depends_on` their children (an epic is done when all children are done). Children do **not** depend on their parent — they depend on sibling prerequisites.

<!-- rule-api:entry id=2de8ee34-b973-41ac-a678-cd5e23a21174 slug=shared/instructions/ticket-system/limit-results/board-coordination/l222 -->
### Board Coordination

The draftboard tracks which agent is working on each ticket and which files
are owned. Check in when starting implementation; check out when done.

<!-- rule-api:entry id=6b675f1d-8bcb-4a89-ae10-89925a4ecf5a slug=shared/instructions/ticket-system/register-yourself-as-actively-working-a-ticket/l227 -->
#### Check-In / Check-Out / Heartbeat

```bash
# Register yourself as actively working a ticket
./target/debug/ticket.exe board check-in <ticket-id> \
  --agent-id <agent-id> \
  --intent "brief description of planned work" \
  --files "src/foo.rs,src/bar.rs" \
  --ttl 3600 \
  --json

<!-- rule-api:entry id=d6979389-ea20-46f2-adf0-a2fddbaf621a slug=shared/instructions/ticket-system/refresh-your-heartbeat-before-ttl-elapses/l238 -->
# Refresh your heartbeat before TTL elapses
./target/debug/ticket.exe board heartbeat <entry-id> --json

<!-- rule-api:entry id=255348e3-9b87-45d3-8d98-a9f9000f2758 slug=shared/instructions/ticket-system/check-out-when-done-records-handoff-reason-in-audit-trail/l241 -->
# Check out when done (records handoff reason in audit trail)
./target/debug/ticket.exe board check-out <ticket-id> \
  --agent-id <agent-id> \
  --reason "implemented and tested" \
  --json
```

<!-- rule-api:entry id=596a1ba8-af91-4a77-9493-7b788d2bdf27 slug=shared/instructions/ticket-system/check-out-when-done-records-handoff-reason-in-audit-trail/wip-limit/l248 -->
#### WIP Limit

`board show` reports `wip_limit_reached` and `next` surfaces a warning when
the limit is hit. Do not start new implementation work when the WIP limit is
reached — finish or hand off an existing entry first.

<!-- rule-api:entry id=dfadc7f4-c524-4346-a2a4-5c1b1f15ddae slug=shared/instructions/ticket-system/check-out-when-done-records-handoff-reason-in-audit-trail/wip-limit/l254 -->
Default limit: 5 simultaneous active entries. Configure:

<!-- rule-api:entry id=b0dcbf8b-6298-4121-a6e4-5947a5adab86 slug=shared/instructions/ticket-system/check-out-when-done-records-handoff-reason-in-audit-trail/wip-limit/l256 -->
```bash
./target/debug/ticket.exe board configure --max-wip 3 --json
```

<!-- rule-api:entry id=5d680076-35d7-4d46-94a6-1b1e51614c5f slug=shared/instructions/ticket-system/check-out-when-done-records-handoff-reason-in-audit-trail/wip-limit/stale-entry-response/l260 -->
#### Stale-Entry Response

An entry becomes **stale** when its heartbeat TTL elapses. `board show`
lists stale entries under `warnings[]` and `stale_count`.

<!-- rule-api:entry id=84e1bcb4-ffc5-4f5f-980d-ccf2441ed1ad slug=shared/instructions/ticket-system/check-out-when-done-records-handoff-reason-in-audit-trail/wip-limit/stale-entry-response/l265 -->
Required responses:
1. Agent still active: run `board heartbeat <entry-id>` to renew.
2. Work abandoned: run `board check-out <ticket-id>` then clean.
3. Remove stale entries: `board clean preview --include-stale`, then
   `board clean apply --token <token> --include-stale`.

<!-- rule-api:entry id=81f313ea-b942-4bf9-885c-70c1f04b987b slug=shared/instructions/ticket-system/check-out-when-done-records-handoff-reason-in-audit-trail/wip-limit/stale-entry-response/file-ownership/l271 -->
#### File Ownership

Owned files block other agents from checking in with overlapping paths.
Keep owned file lists narrow and release them (via check-out or update-files)
when no longer needed.

<!-- rule-api:entry id=acee85b1-1af3-4299-907d-05b3426e367b slug=shared/instructions/ticket-system/add-remove-files-from-an-active-entry/l277 -->
```bash
# Add / remove files from an active entry
./target/debug/ticket.exe board update-files <ticket-id> \
  --agent-id <agent-id> --add "new.rs" --remove "old.rs" --json

<!-- rule-api:entry id=4b1bb767-4cb4-4786-b94a-94c59dd128aa slug=shared/instructions/ticket-system/rename-a-file-in-an-active-entry-atomic/l282 -->
# Rename a file in an active entry (atomic)
./target/debug/ticket.exe board rename-file <ticket-id> \
  --agent-id <agent-id> --old-path "old.rs" --new-path "new.rs" --json
```

<!-- rule-api:entry id=5a73d014-bdef-4b77-845f-5dedd86b805e slug=shared/instructions/ticket-system/rename-a-file-in-an-active-entry-atomic/dependency-maintenance/l287 -->
### Dependency Maintenance

After completing significant work, check whether finished tickets unblock others and update those links:

<!-- rule-api:entry id=79072d96-8a70-4e68-ae7f-56dc40c75955 slug=shared/instructions/ticket-system/find-what-a-completed-ticket-blocks/l291 -->
```bash
# Find what a completed ticket blocks
./target/debug/ticket.exe topgraph <id> --json \
  | jq -r '.payload.nodes[] | select(.state=="new" or .state=="ready") | .id'
```

<!-- rule-api:entry id=5d01592b-51e9-4b38-90a2-ef408e0039ee slug=shared/instructions/ticket-system/find-what-a-completed-ticket-blocks/l297 -->
Add missing `depends_on` edges when you discover undocumented dependencies. Use `--reason` on every link to explain *why* the dependency exists.

<!-- rule-api:entry id=918126ee-5392-4b5f-b8ee-9bc9e78d7ab1 slug=shared/instructions/ticket-system/find-what-a-completed-ticket-blocks/commit-checkpoint-suggestions/l299 -->
### Commit Checkpoint Suggestions

Suggest a `git commit` checkpoint to the user when any of the following is true:

<!-- rule-api:entry id=346f3851-513b-49ec-969a-c45ad6189f18 slug=shared/instructions/ticket-system/find-what-a-completed-ticket-blocks/commit-checkpoint-suggestions/l303 -->
- A ticket transitions to `closed` (work milestone reached).
- A batch of related tickets all reach `closed` or `in-implementation` together.
- A dependency graph changes materially (new links added/removed).
- A tracked bug is fixed and its ticket closed.

<!-- rule-api:entry id=013ab56e-1a29-4f22-a249-77ffad4a04ad slug=shared/instructions/ticket-system/find-what-a-completed-ticket-blocks/commit-checkpoint-suggestions/l308 -->
Phrase suggestions like:

<!-- rule-api:entry id=902d95a1-4aa7-42d0-99d9-e8c506f81a75 slug=shared/instructions/ticket-system/find-what-a-completed-ticket-blocks/commit-checkpoint-suggestions/l310 -->
> "Ticket `<title>` is now closed — good checkpoint for a commit. Suggested message: `<imperative summary of what was done>`."

<!-- rule-api:entry id=4f2a8a41-8f00-4c77-8776-c805902db3c6 slug=shared/instructions/ticket-system/find-what-a-completed-ticket-blocks/commit-checkpoint-suggestions/aggressive-quality-improvement/l312 -->
### Aggressive Quality Improvement

Opportunistically improve ticket quality whenever you touch the store:

<!-- rule-api:entry id=e3678895-bd51-438c-b501-2111a47996ac slug=shared/instructions/ticket-system/find-what-a-completed-ticket-blocks/commit-checkpoint-suggestions/aggressive-quality-improvement/l316 -->
- Fill in missing `description`, `priority`, or `type` fields on tickets you encounter.
- Split vague tickets into concrete, actionable child tickets linked with `depends_on`.
- Remove or merge duplicate tickets.
- Verify that `in-implementation` tickets actually have an active owner/context; flag stale ones.
- After any structural refactor, re-run `ticket health --all` and resolve reported issues.

<!-- rule-api:entry id=b5e0ebf0-20ef-4eec-b03e-7ddc987ad2c9 slug=shared/instructions/ticket-system/find-what-a-completed-ticket-blocks/workflow-expectations/l322 -->
## Workflow Expectations

- For larger ticket-system work, start with ticket planning workflow first.
- Search existing tickets/issues before introducing new behavior.
- Record acceptance criteria for behavior changes.

<!-- rule-api:entry id=f6b52b7e-d9d4-4743-bd2d-9f21b9c71689 slug=shared/instructions/ticket-system/health-check-a-subgraph-rooted-at-a-ticket-bfs-traversal/l330 -->
### Health Checks

```bash
# Health-check a subgraph rooted at a ticket (BFS traversal)
ticket health abcd1234 --json

<!-- rule-api:entry id=8e515eb1-f418-4030-aac2-576ea9c90997 slug=shared/instructions/ticket-system/health-check-a-subgraph-filtering-to-a-specific-type/l342 -->
# Health-check a subgraph, filtering to a specific type
ticket health abcd1234 --where type=tracker-improvement --json
```

<!-- rule-api:entry id=2c8ceae2-0b22-40e0-82f2-069712654a59 slug=shared/instructions/ticket-system/health-check-all-tickets/l336 -->
# Health-check all tickets
ticket health --all --json

<!-- rule-api:entry id=6dbc4c74-3f59-40bf-9a22-19b04087fd63 slug=shared/instructions/ticket-system/health-check-all-new-tickets-where-filter/l339 -->
# Health-check all new tickets (--where filter)
ticket health --all --where state=new --json

<!-- rule-api:entry id=5b51b39b-44c4-4241-bb06-2e0fc3293e6d slug=shared/instructions/ticket-system/list-tickets-pipe-ids-health-check/l346 -->
### Command Chaining (pipe via --stdin)

```bash
# List tickets → pipe IDs → health check
ticket list --where priority=high --json \
  | jq -r '.payload.items[].id' \
  | ticket health --stdin --json

<!-- rule-api:entry id=357ae6d0-b9ce-464f-8e8e-8b2d061af3a6 slug=shared/instructions/ticket-system/subgraph-filter-new-tickets-health-check/l354 -->
# Subgraph → filter new tickets → health check
ticket subgraph abcd1234 --json \
  | jq -r '.payload.nodes[] | select(.state=="new") | .id' \
  | ticket health --stdin --json

<!-- rule-api:entry id=695424d3-bf97-4fc1-bf47-66077eed9609 slug=shared/instructions/ticket-system/topgraph-health-check-all-reverse-dependencies/l359 -->
# Topgraph → health check all reverse dependencies
ticket topgraph abcd1234 --json \
  | jq -r '.payload.nodes[].id' \
  | ticket health --stdin --json
```

<!-- rule-api:entry id=f5d1cf44-dfe0-4cb7-9c7c-8fe3d4c3f692 slug=shared/instructions/ticket-system/topgraph-health-check-all-reverse-dependencies/batch-cli-syntax-transactional/l365 -->
### Batch (CLI-syntax, transactional)

`ticket batch` reads one CLI command per line from stdin (or `--file`). All
commands execute against the same store in order. If any command fails, all
prior writes are rolled back automatically. Blank lines and `#` comments are
ignored.

<!-- rule-api:entry id=8eb22e0e-3fa2-4405-ab06-2c8e120e1a08 slug=shared/instructions/ticket-system/from-a-checked-in-batch-file/l381 -->
# From a checked-in batch file
ticket batch --file scripts/bootstrap-tickets.txt --json

<!-- rule-api:entry id=66cdb609-7ed7-4d4e-bbbb-0799f188dbbd slug=shared/instructions/ticket-system/heredoc-create-tickets-link-all-atomic/l372 -->
```bash
# Heredoc — create tickets + link, all atomic
ticket batch --json <<'EOF'
create --title "Extract GPU pipeline" --type tracker-improvement
create --title "Add shader cache" --type tracker-improvement
# link is resolved after creates succeed
link --from <UUID-A> --to <UUID-B> --kind depends_on
EOF

<!-- rule-api:entry id=28f59839-73ec-4570-9b03-fd2cc4bcbd6e slug=shared/instructions/ticket-system/stdin-from-another-process/l384 -->
# Stdin from another process
echo -e "create --title 'Setup CI' --type tracker-improvement\nclose <UUID>" \
  | ticket batch --json
```

<!-- rule-api:entry id=1bf98739-fed0-417c-a571-38a51b192527 slug=shared/instructions/ticket-system/stdin-from-another-process/l389 -->
**Rules:**
- Each line is parsed identically to a top-level `ticket <subcommand>` call.
- `serve`, `watch`, nested `batch`, `scan`, lease commands, and
  config commands (`add-root`, `workspace`, `export-command-schema`) are
  rejected with a clear error — they cannot be used inside a batch.
- On rollback: `create` → deleted, `update` → state/fields restored, `link`
  → edge removed. Read-only commands (`get`, `list`, `search`, `health`, etc.)
  produce no undo entry and are not affected by rollback.
- No `--index-root` requirement — uses normal workspace resolution like any
  other CLI command.

<!-- rule-api:entry id=671ecb92-d75a-4849-8671-d5c8307d4cab slug=shared/instructions/ticket-system/stdin-from-another-process/http-endpoints/l400 -->
## HTTP Endpoints

```
GET /api/graph/subgraph?workspace=default&root=<UUID>&depth=2
GET /api/graph/topgraph?workspace=default&root=<UUID>&depth=2
GET /api/graph/health?workspace=default&all=true
GET /api/graph/health?workspace=default&root=<UUID>&depth=4&direction=out
```

<!-- rule-api:entry id=b1432ed4-e1d7-49bb-9c30-b1534daa9b9b slug=shared/instructions/ticket-system/stdin-from-another-process/index-reconciliation-scan-force/l409 -->
## Index Reconciliation (`scan --force`)

`scan` normally only integrates new/changed files it discovers. Use
`scan --force` to force a full reconciliation — every ticket.toml is re-read
from disk and both the SQLite index and Tantivy search index are rebuilt:

<!-- rule-api:entry id=dda5ff60-8330-4de0-823d-aad37cb5f2cd slug=shared/instructions/ticket-system/force-reconcile-all-indexes-from-disk/l415 -->
```bash
# Force-reconcile all indexes from disk
./target/debug/ticket.exe scan --force --json
```

<!-- rule-api:entry id=c523da84-89f9-4b30-9558-198351cae905 slug=shared/instructions/ticket-system/force-reconcile-all-indexes-from-disk/l420 -->
Output includes `"force": true` and `"reconciled": <count>` showing how many
tickets were re-indexed. Use this after manual edits to ticket.toml files or
when the index seems stale.

<!-- rule-api:entry id=6d1e303c-8095-4534-9d9e-c0067ef78bda slug=shared/instructions/ticket-system/force-reconcile-all-indexes-from-disk/validation/l424 -->
## Validation

- Prefer focused tests for changed modules before broader suites.
- Verify search/index behavior when touching ticket query paths.
- Confirm no regressions in CLI or MCP-facing flows for changed endpoints.
