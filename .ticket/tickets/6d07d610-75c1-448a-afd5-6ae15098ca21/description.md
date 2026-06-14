# [ticket-vscode] Rust/WASM port track

Port `memory-viewers/memory-api/tools/ticket-vscode` from a TypeScript-heavy implementation to a Rust/WASM-backed VS Code extension architecture.

The target is a dual-host extension, not a JS-free extension. VS Code still requires JS entrypoints for activation and API access. The migration should therefore:

- keep a thin JS/TS host shell for VS Code integration
- move deterministic domain and tree-model logic into a Rust/WASM core
- redesign or explicitly scope desktop-only features that depend on Node/Electron behavior
- validate the result in desktop, web, and remote-oriented scenarios

Planning spec:
- `ticket-vscode/rust-wasm-port` (`a592900c-f513-4ec2-8dd2-53dbd04aac7b`)

Research summary:
- The current extension mixes portable logic (`src/api.ts`, parts of `src/ticketProvider.ts`) with Node-bound behavior (`src/extensionSupport.ts`, `src/browserBridge.ts`, parts of `src/extensionCommands.ts`).
- VS Code web extensions require a `browser` entry, a WebWorker-compatible runtime, and a single-file bundle.
- Browser/web hosts cannot use `child_process`, raw `fs/path/process` access, or local CDP/browser automation.
- Remote-safe behavior should prefer `vscode.env.openExternal`, `vscode.env.asExternalUri`, and `vscode.env.clipboard`.

## Execution order

The port track executes in this order, and the tracker depends on every child ticket below:

1. [93f7e422 Freeze Rust/WASM architecture spec and feature matrix](../93f7e422-1e41-4145-b8ba-0dcf7fc730ac/ticket.toml)
2. [14047b99 Prove dual-host WASM activation](../14047b99-41d6-4899-bec6-4a919bffcc2d/ticket.toml)
3. [011563c2 Extract portable Rust core for ticket/domain logic](../011563c2-59e7-48f1-a61f-d8fdc80d2f6e/ticket.toml)
4. [bfafde19 Replace Node-bound behaviors with host capability adapters](../bfafde19-ddf7-47ef-966e-a1135be4efd6/ticket.toml)
5. [362448d4 Add dual-host packaging, bundling, and extension test harnesses](../362448d4-ccf1-4b9d-90f3-d4577da83a65/ticket.toml)
6. [6de424b0 Validate Rust/WASM parity across desktop, web, and remote hosts](../6de424b0-68ec-43c7-9d70-eb8d17305ab3/ticket.toml)

The sequential dependency chain is:

- [14047b99 Prove dual-host WASM activation](../14047b99-41d6-4899-bec6-4a919bffcc2d/ticket.toml) depends on [93f7e422 Freeze Rust/WASM architecture spec and feature matrix](../93f7e422-1e41-4145-b8ba-0dcf7fc730ac/ticket.toml)
- [011563c2 Extract portable Rust core for ticket/domain logic](../011563c2-59e7-48f1-a61f-d8fdc80d2f6e/ticket.toml) depends on [14047b99 Prove dual-host WASM activation](../14047b99-41d6-4899-bec6-4a919bffcc2d/ticket.toml)
- [bfafde19 Replace Node-bound behaviors with host capability adapters](../bfafde19-ddf7-47ef-966e-a1135be4efd6/ticket.toml) depends on [011563c2 Extract portable Rust core for ticket/domain logic](../011563c2-59e7-48f1-a61f-d8fdc80d2f6e/ticket.toml)
- [362448d4 Add dual-host packaging, bundling, and extension test harnesses](../362448d4-ccf1-4b9d-90f3-d4577da83a65/ticket.toml) depends on [bfafde19 Replace Node-bound behaviors with host capability adapters](../bfafde19-ddf7-47ef-966e-a1135be4efd6/ticket.toml)
- [6de424b0 Validate Rust/WASM parity across desktop, web, and remote hosts](../6de424b0-68ec-43c7-9d70-eb8d17305ab3/ticket.toml) depends on [362448d4 Add dual-host packaging, bundling, and extension test harnesses](../362448d4-ccf1-4b9d-90f3-d4577da83a65/ticket.toml)

This parent ticket is done when all child tickets are done, the spec reflects the final architecture and validation evidence, and the port plan has been executed with explicit host capability rules.
