## Problem
`main.rs` spawns the child directly from its resolved PATH location, which holds a Windows file lock (`os error 5`) on `~/.cargo/bin/<server>.exe` for the process lifetime, blocking `cargo install --force`. Part of epic 25780944; depends on T1 (rename), T2 (policy split, so this slots into the lifecycle-neutral core).

## Approach
On startup, resolve the child binary's canonical path P (from the `--` args, PATH lookup). Copy P to a private shadow path S under a temp dir keyed by name+pid+hash (e.g. `%TEMP%/mcp-toolmon/<name>-<pid>-<hash>/<exe>`). Spawn S instead of P. Clean up shadow copies on normal exit. This alone fixes the Windows lock problem even without reload (T4-T6).

## Acceptance criteria
- [ ] Canonical child path P resolved once at startup (PATH lookup honoring existing arg-parsing in `main.rs`)
- [ ] Shadow copy created at a private path keyed by name+pid+hash before spawn
- [ ] Child process spawned from the shadow path S, not P
- [ ] P is never open/locked by mcp-toolmon at any point after the copy completes (verified: `cargo install --force` targeting P succeeds while mcp-toolmon runs)
- [ ] Shadow directory removed on graceful process exit; stale shadow dirs from crashed processes do not accumulate unbounded (best-effort cleanup or documented TTL)
- [ ] `TOOLMON_SHADOW_DIR` env var overrides the default shadow root (used later by T6 too, defined here since this ticket owns shadow-copy mechanics)
- [ ] Existing spawn behavior (piped stdin/stdout, inherited stderr) preserved
- [ ] Unit test: shadow copy is created and spawned binary's actual path differs from P
- [ ] Integration/manual note: Windows lock scenario documented as verified (can be deferred to T7 for full automated coverage)

## Files touched
- memory-api/tools/mcp/mcp-toolmon/src/main.rs
- memory-api/tools/mcp/mcp-toolmon/src/shadow.rs (new)
- memory-api/tools/mcp/mcp-toolmon/tests/ (new shadow-copy unit tests)


## Validation acceptance criteria (addendum)
- [ ] Unit test `shadow_copy_spawns_from_shadow_path`: after startup, the child process's own reported/observed executable path differs from canonical path P (e.g. assert via a fixture binary that echoes its own `std::env::current_exe()`)
- [ ] Unit test `shadow_dir_env_override`: setting `TOOLMON_SHADOW_DIR` to a temp dir causes the shadow copy to be created under that dir, not the default
- [ ] Unit test `startup_sweep_removes_dead_shadow`: a shadow artifact whose owning pid is not alive is deleted on next startup sweep; a shadow artifact whose pid IS alive is retained
- [ ] Integration test `windows_lock_freedom` (`#[cfg(windows)]` in `tests/windows_lock.rs`): while mcp-toolmon runs a child spawned from a shadow copy of a temp "canonical" path P, the test process itself renames/overwrites P and asserts no OS lock error (previously reproduced as `os error 5`); test is absent (not `#[ignore]`) on non-Windows targets
- [ ] `cargo test -p mcp-toolmon` includes the above three unit tests plus (on Windows) `windows_lock_freedom`, all passing
## T3 completion note

Implemented shadow-copy execution (`src/shadow.rs`) + fixture bins for the shadow/reload test matrix.

- Canonical resolution: bare names go through PATH (+PATHEXT on Windows); path-like args are canonicalized directly. `resolve_canonical()`.
- Shadow copy: `make_shadow_copy()` copies P into `<root>/<name>-<pid>-<hash>/<exe>`; `Supervisor::spawn`/`spawn_with_shadow_dir` spawn the shadow exe, falling back to P with a stderr log on any copy/resolution failure (never fails startup).
- Startup sweep: `sweep_startup()` deletes shadow dirs whose encoded pid is dead (checked via `/proc/<pid>` or `kill -0` on unix, `tasklist` on Windows); best-effort, never fatal. No TTL, per spec R12.
- Fixture crate deviation (IMPORTANT): the spec calls for a separate `tests/fixtures/fake-mcp` workspace-member crate with `fake-mcp-v1`/`v2` bins consumed via `env!("CARGO_BIN_EXE_fake-mcp-v1")`. Empirically verified that `CARGO_BIN_EXE_<name>` is only set by Cargo for bin targets of the package under test, never for a dependency crate's bins (confirmed: adding the fixture as a path dev-dependency compiled fine but left `CARGO_BIN_EXE_fake-mcp-v1` undefined at test-compile time). Fix: `fake-mcp-v1`/`fake-mcp-v2` are declared as `[[bin]]` targets of `mcp-toolmon` itself, sourced from `tests/fixtures/fake-mcp/src/bin/*.rs` (still two genuinely separate, byte-different source files). This makes the env vars resolve for real (verified) at the cost of not being an isolated workspace member — flag for T4/T6/T7, which depend on this fixture.
- Tests: `shadow_copy_spawns_from_shadow_path`, `shadow_dir_env_override`, `startup_sweep_removes_dead_shadow`, `windows_lock_freedom` (`#[cfg(windows)]`) all added and passing in `tests/shadow.rs`. 59/59 total (55 prior + 4 new).
- Smoke-verified: piped `initialize` through `mcp-toolmon -- peek-mcp`, got a real result payload, and observed the shadow copy on disk at `/tmp/mcp-toolmon/peek-mcp-25404-b5bee4bb59256985/peek-mcp.exe`.
- Not implemented: on-graceful-exit shadow-dir deletion (ticket's pre-addendum AC mentions it; spec R12 only requires startup-sweep + post-swap supersession, no exit-time cleanup, so left out to match spec normative text; startup sweep bounds accumulation instead).


## Verification note (2026-07-31)
cargo test -p mcp-toolmon -p toolmon-policy-api -p toolmon-costgate -> 76/76 passed, run twice, 0 flakes. Property A (proxied target-server functionality not compromised) and Property B (child hot-restart) both PROVEN. Full evidence and known limitations recorded on epic 25780944.