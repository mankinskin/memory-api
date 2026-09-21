## Problem
`tests/crash_inflight_subprocess.rs` (T7, real mcp-toolmon subprocess, real OS-level child kill mid-flight) empirically observed: after the automatic crash-recovery respawn (R7) completes and successfully serves a follow-up request, closing the client's stdin (triggering `Supervisor::shutdown()`) takes ~89 seconds for the mcp-toolmon PROCESS itself to exit via `child.wait()`, versus ~2 seconds for the equivalent clean-swap shutdown path in `integration_reload_end_to_end.rs`. No request is ever dropped and the client connection is never severed (R6/R7 hold), but this is a real, reproducible latency anomaly specific to the crash-then-shutdown sequence, observed on Windows.

## Evidence
- Test: memory-api/tools/mcp/mcp-toolmon/tests/crash_inflight_subprocess.rs::crash_mid_flight_every_id_answered
- Instrumented timestamps show all in-flight ids (10,11,12) and the post-recovery health check (id=99) answered within ~10ms of the kill, but child.wait() after drop(stdin) did not return for ~89s.
- Suspected area: Supervisor::shutdown() in src/supervisor.rs waits on child.wait() for the CURRENT (post-crash-recovery-respawned) child after only closing its stdin; something about the crash-recovery-respawned generation appears to delay the child's own exit-on-EOF compared to a child spawned via the normal watcher-swap path.

## Suggested next step
Reproduce in isolation (bypass the test harness's PowerShell child-pid lookup) to rule out that lookup mechanism as a contributor, then trace whether the respawned child's stdin pipe is actually closing at the OS level promptly, and whether tokio::process::Child::wait() polling behaves differently for a child spawned during the crash-recovery branch of swap_child_with_drain_ms vs the normal branch.

## Acceptance criteria
- [ ] Root cause identified for the shutdown latency differential between crash-recovery and normal-swap shutdown paths
- [ ] Fix applied or explicit justification recorded if the latency is inherent/unavoidable
- [ ] Regression test added asserting shutdown-after-crash-recovery completes within a bounded time (e.g. <5s)



## Root-cause finding (2026-07-31)

Supervisor::shutdown at memory-api/tools/mcp/mcp-toolmon/src/supervisor.rs#L789-L797 closes the current child's stdin and then awaits child.wait() with no timeout. After a crash-recovery cycle the child can linger, producing the observed ~89s delay.\n\nThis path is reached only on whole-process exit (EOF on the proxy's own stdin) — it is NOT on the child hot-swap path and is never hit in a normal session where only child servers reload. Symptom recorded in tests/crash_inflight_subprocess.rs#L192-L194, where the test force-kills the proxy and cites this ticket.\n\nFix sketch: bound the shutdown — close child stdin, then start_kill() plus a timeout around wait(), keeping the current graceful wait only as a fast path when the child has already exited.\n\nLeft open/tracked as follow-up; not fixed by this pass. Epic 25780944 (mcp-toolmon reload epic) is done; this ticket is intentionally separate.