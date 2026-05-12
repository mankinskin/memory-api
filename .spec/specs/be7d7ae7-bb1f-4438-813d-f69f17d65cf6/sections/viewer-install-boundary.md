# Viewer Installation Boundary

| ID | Scenario | Status | Commands | Assertions |
| --- | --- | --- | --- | --- |
| VIEW-01 | Install `viewer-ctl` itself | Required first executable viewer gate | `cargo install --path memory-viewers/viewer-api/viewer-ctl --bin viewer-ctl` | `viewer-ctl --help` succeeds and the installed binary can inspect configured viewers |
| VIEW-02 | Install the baseline managed viewers (`doc-viewer` and `log-viewer`) | Required first executable viewer slice; expand to the remaining viewers after this gate is stable | `viewer-ctl install doc-viewer --kind server`, `viewer-ctl install doc-viewer --kind frontend`, `viewer-ctl install log-viewer --kind server`, `viewer-ctl install log-viewer --kind frontend` | `doc-viewer` and `log-viewer` are installed into Cargo's bin dir, `~/.context-engine/static/doc-viewer/index.html` and `~/.context-engine/static/log-viewer/index.html` exist, and the install commands are repeatable |
| VIEW-03 | Prepare/start/stop lifecycle smoke | Follow-up after the representative install slice is covered | `viewer-ctl prepare <viewer>`, `viewer-ctl start <viewer>`, `viewer-ctl stop <viewer>` | One representative viewer starts from a clean environment and can be stopped cleanly |
| VIEW-04 | Viewer deinstall ergonomics | Explicit follow-up; not in the first gating slice | No first-class uninstall command exists in `viewer-ctl` today | The design must record the current gap and decide whether uninstall becomes a supported lifecycle command |

## Current Gap

The first executable viewer gate covers `viewer-ctl` plus the baseline managed viewers `doc-viewer` and `log-viewer`. Matrix expansion for `ticket-viewer` and `spec-viewer` remains follow-up work after the baseline slice is stable.

`viewer-ctl` currently exposes `install`, `build`, `prepare`, `start`, `stop`, and `restart`, but no explicit `uninstall` or `remove` command. Viewer deinstall coverage is therefore a planned follow-up instead of a required first implementation gate.
