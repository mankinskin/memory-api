# [ticket-vscode] Replace Node-bound behaviors with host capability adapters

Refactor the extension host layer so runtime-specific behavior is isolated behind explicit capabilities instead of being embedded throughout `extensionSupport.ts`, `extensionCommands.ts`, and `ticketProvider.ts`.

This includes redesigning or scoping these current behaviors:
- `.ticket` workspace discovery via raw `fs/path`
- on-disk ticket file browsing in the tree
- server launch and binary discovery via `child_process` and `process.env`
- local preferred-browser launching
- Browser Bridge / CDP automation

Acceptance criteria:
- [ ] Runtime-sensitive behavior is moved behind capability adapters or split host-specific modules.
- [ ] File and folder browsing uses VS Code URI/workspace APIs where supported and defines fallback behavior for virtual workspaces.
- [ ] `openExternal`, `asExternalUri`, and `env.clipboard` replace local-process assumptions where appropriate for remote/browser safety.
- [ ] Commands or features that remain desktop-only are hidden, gated, or explained explicitly instead of failing at runtime.
