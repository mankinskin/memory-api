# Viewer Installation Boundary

| ID | Scenario | Status | Commands | Assertions |
| --- | --- | --- | --- | --- |
| VIEW-01 | Install `viewer-ctl` itself | Required first executable viewer gate | `cargo install --path memory-viewers/viewer-api/viewer-ctl --bin viewer-ctl` | `viewer-ctl --help` succeeds and the installed binary can inspect configured viewers |
| VIEW-02 | Install all managed viewers (`doc-viewer`, `log-viewer`, `ticket-viewer`, and `spec-viewer`) | Required managed-viewer install gate | `viewer-ctl install doc-viewer --kind server`, `viewer-ctl install doc-viewer --kind frontend`, `viewer-ctl install log-viewer --kind server`, `viewer-ctl install log-viewer --kind frontend`, `viewer-ctl install ticket-viewer --kind server`, `viewer-ctl install ticket-viewer --kind frontend`, `viewer-ctl install spec-viewer --kind server`, `viewer-ctl install spec-viewer --kind frontend` | All managed viewer binaries are installed into Cargo's bin dir, `~/.context-engine/static/<viewer>/index.html` exists for each managed viewer, and the install commands are repeatable |
| VIEW-03 | Prepare/start/stop lifecycle smoke | Follow-up after the managed-viewer install gate is covered | `viewer-ctl prepare <viewer>`, `viewer-ctl start <viewer>`, `viewer-ctl stop <viewer>` | One representative viewer can be prepared from a clean environment, started with its configured static dir and port, and then stopped cleanly without removing the installed artifacts |
| VIEW-04 | Viewer deinstall ergonomics | Follow-up tracked by ticket `6bbda148-e144-4dff-92de-dd6584c82bd7` | No first-class uninstall command exists in `viewer-ctl` today | Implement a supported uninstall/remove command, then extend the contract and Docker harness to assert viewer deinstall behavior |

## Current Gap

The first executable viewer gate now covers `viewer-ctl` plus all managed viewers in `viewer-ctl.toml`: `doc-viewer`, `log-viewer`, `ticket-viewer`, and `spec-viewer`.

Remaining follow-up work is lifecycle smoke coverage (`VIEW-03`) and explicit viewer deinstall ergonomics (`VIEW-04`).

`viewer-ctl` currently exposes `install`, `build`, `prepare`, `start`, `stop`, and `restart`, but no explicit `uninstall` or `remove` command. Viewer deinstall coverage is therefore a planned follow-up instead of a required first implementation gate.

## VIEW-03 Lifecycle Definition

For this install contract, the `VIEW-03` lifecycle covers the first runnable viewer path after installation:

1. `viewer-ctl prepare <viewer>` materializes the viewer's static assets or other startup prerequisites without starting a long-lived process.
2. `viewer-ctl start <viewer>` launches the configured server for that viewer from a clean environment and points it at the installed static directory.
3. `viewer-ctl stop <viewer>` terminates the started server process cleanly while leaving the installed artifacts in place.

The lifecycle check is intentionally separate from deinstall coverage: `VIEW-03` proves that an installed viewer can be prepared, started, and stopped, while `VIEW-04` will cover artifact removal once `viewer-ctl` exposes an uninstall/remove command.

## VIEW-04 Follow-up

Implementation of the viewer uninstall/remove command is tracked by ticket `6bbda148-e144-4dff-92de-dd6584c82bd7` (`[viewer-ctl] Implement uninstall command for managed viewers`).
