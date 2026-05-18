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

This parent ticket is done when all child tickets are done, the spec reflects the final architecture and validation evidence, and the port plan has been executed with explicit host capability rules.
