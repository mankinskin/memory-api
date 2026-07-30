Renamed ticket lifecycle states `new`->`open` and `ready`->`planned` across the schema, code, and both real ticket stores.

Files changed (schema/code):
- memory-api/crates/ticket-api/schemas/{bug,epic,feature,task,tracker-improvement}.toml: states/transitions renamed (schema source of truth).
- memory-api/crates/ticket-api/src/{health.rs,store_index.rs,storage/store.rs,storage/store/lifecycle.rs,storage/move_planner.rs,storage/ticket_fs/tests.rs,storage/tests/*.rs,watcher/reconciler.rs,workflow/workflow/tests.rs,missing_rule.rs}: literal state defaults/tests renamed; health.rs off-schema test now uses "archived" as the off-schema example.
- memory-api/crates/memory-api/src/{index_generator/ticket.rs,storage/entity_fs.rs,storage/entity_store.rs,storage/search_tests.rs,model/manifest_format/tests.rs}: renamed.
- memory-api/tools/cli/ticket-cli/src/cli/**, tools/http/ticket-http/src/**, tools/mcp/ticket-mcp/src/**, plus their tests: renamed literals; human_output.rs assertions fixed to match.
- memory-api/crates/session-api/src/follow_up.rs, crates/memory-matrix/**, crates/ticket/tests, crates/audit-api/src/trials/ticket_graph/tests.rs: renamed (ticket-domain literals only; feedback-api's unrelated FeedbackStatus::New and memory-matrix's criterion-bench "new" folder were left untouched).
- context-stack/tools/context-editor/kernel/src/ui/ticket_editor.rs (+tests.rs): second state-enum definition found; TicketState::New/Ready renamed to Open/Planned via symbol rename, from_str/as_str literals updated.
- memory-viewers/ticket-viewer/frontend/dioxus/src/**, memory-viewers/ticket-viewer/src/main.rs: renamed, including UI chip/label text.
- memory-api/tools/ticket-vscode/src/{extension.ts,extensionSupport.ts,ticketTreeItems.ts} and test/unit/buildStateGroups.test.ts: renamed.
- memory-api/crates/ticket-vscode-core/src/lib.rs: renamed.
- memory-api/crates/memory-fixtures/src + memory-api/test-fixtures/**: renamed (this local crate/fixture is unused dead code — all real consumers pull `memory-fixtures` via git dependency from github.com/mankinskin/memory-fixtures, so memory-api/crates/ticket-api/tests/e2e_fixture_loader.rs assertion was reverted to expect the external repo's still-"new" fixture data, which is out of this repo's control).
- .agents/instructions/ticket/lifecycle.instructions.md, .agents/instructions/ticket/workflow.instructions.md, .agents/prompts/swarm-worker.prompt.md: guidance prose updated.

Migration: ran a targeted Python script (regex substring rewrite of `state = "new"/"ready"` in ticket.toml and `"state":"new"/"ready"` in history.ndjson only, no JSON re-serialization) against both `.ticket` (repo root) and `memory-api/.ticket`.
- Dry-run: reported 356/96 ticket.toml and 779/301 history.ndjson would change; verified zero files written.
- Real run: same counts applied.
- Re-run (idempotence, AC4): 0/0 changes both stores.
- Byte-identity (AC3): diffed migrated history.ndjson against pre-migration backups line-by-line; 2997 (root) + 1086 (memory-api) lines changed, 0 unexpected mismatches — every changed line differs only in the state token.

Validation: `cargo test -p ticket-api` 143+1+1+4 passed, 0 failed. `cargo build --workspace` succeeds (warnings only). AC1 verified manually: `open->planned` accepted; `open->ready` rejected with `allows next states [cancelled, planned]`. AC8: 8 test-api executions recorded (exec-5b3da351-ac1..ac8) linked to this ticket and to validation spec vt-5b3da351-ticket-state-rename.

Known gap (AC7, recorded as `blocked` in exec-5b3da351-ac7-guidance-sweep): `.rule/README.md` (generated rule catalog) still contains 2 stale lines from rule.toml entries whose backing files are no longer discoverable in any `.rule` store; needs a rule-cli sync/regeneration outside this ticket's scope to hand-edit a generated artifact.

Known pre-existing, unrelated gaps observed (not caused by this change, not fixed): ticket-cli's `integration_store_index` tests fail due to a missing `--description-mode` flag (gap from ticket 3d952036, unrelated to state rename); ticket-http/ticket-mcp have several workspace-registry tests failing with "workspace should open"/"workspace should initialize" that reproduce even after stashing all my changes back to a state with concurrent uncommitted work from another agent — confirmed unrelated to the state rename.</description>
<parameter name="description_mode">replace