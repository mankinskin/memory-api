# [ticket-vscode] Add dual-host packaging, bundling, and extension test harnesses

Once the runtime spike and capability boundary are settled, package the extension so the same project can run in both desktop and browser-compatible hosts.

Acceptance criteria:
- [ ] `package.json` exposes the required `main` and `browser` entrypoints and any needed `extensionKind` guidance.
- [ ] Build scripts generate the desktop bundle, the single-file web extension bundle, and the associated WASM artifact/glue consistently.
- [ ] Launch/debug tasks cover a desktop run and a web-extension-host run.
- [ ] Automated smoke coverage exists for extension activation in at least one desktop-host path and one `@vscode/test-web` path.
- [ ] The packaging docs describe how to build, run, and troubleshoot the Rust/WASM-backed extension locally.
