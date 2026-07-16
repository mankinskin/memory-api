Completed the session-api store module hierarchy normalization.

- Moved active SessionStoreConfig implementation slices under `crates/session-api/src/store/config/` and removed the transitional `store_config_methods` and inactive `store_config_impl` trees.
- Reorganized extracted store tests under capture, worktree, runtime, workflow, and finish domain folders while retaining `store_tests.rs` as a thin include orchestrator.
- Consolidated active persistence helpers under `store/helpers/storage.rs` and removed the unused divergent `store_helpers_events.rs` implementation.
- Validation: `rtk cargo test -p session-api --lib` passed with 99 tests; `rtk git diff --check` passed; touched files have no editor diagnostics.

No behavioral requirement changed, so no new spec was created.