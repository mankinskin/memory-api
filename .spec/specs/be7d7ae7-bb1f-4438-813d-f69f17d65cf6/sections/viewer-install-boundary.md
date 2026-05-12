# Viewer Installation Boundary

| ID | Scenario | Status | Commands | Assertions |
| --- | --- | --- | --- | --- |
| VIEW-01 | Install `viewer-ctl` itself | Required design decision; implementation may follow CLI completion | `cargo install --path memory-viewers/viewer-api/viewer-ctl --bin viewer-ctl` | `viewer-ctl --help` succeeds |
| VIEW-02 | Install viewer server artifacts | Follow-up after CLI matrix is stable | `viewer-ctl install doc-viewer`, `viewer-ctl install log-viewer`, `viewer-ctl install ticket-viewer`, `viewer-ctl install spec-viewer` | Viewer artifacts are installed repeatably in the expected locations |
| VIEW-03 | Prepare/start/stop lifecycle smoke | Follow-up after install lifecycle is covered | `viewer-ctl prepare <viewer>`, `viewer-ctl start <viewer>`, `viewer-ctl stop <viewer>` | One representative viewer starts from a clean environment and can be stopped cleanly |
| VIEW-04 | Viewer deinstall ergonomics | Explicit follow-up; not in the first gating slice | No first-class uninstall command exists in `viewer-ctl` today | The design must record the current gap and decide whether uninstall becomes a supported lifecycle command |

## Current Gap

`viewer-ctl` currently exposes `install`, `build`, `prepare`, `start`, `stop`, and `restart`, but no explicit `uninstall` or `remove` command. Viewer deinstall coverage is therefore a planned follow-up instead of a required first implementation gate.
