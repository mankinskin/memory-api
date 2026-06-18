## Quality Gates

- Relevant validation must pass before completion. If a required check repeatedly fails, stop expanding scope and record the failing command, log or manual result, and blocker clearly in the ticket/spec status summary.
- Before a ticket moves to `in-review`, ensure the relevant spec is updated for the changed requirements or goals and links the related tickets, updated docs, and test or validation results.
- **Browser verification is mandatory** for any change to a server interface or frontend feature:
  open the affected viewer in an external fullscreen Chromium-family browser, not VS Code's integrated browser, and confirm the feature works visually before marking work done.
- Record the browser window or display resolution used for manual visual validation whenever layout, rendering, or responsive behavior could affect the result.
- **Write Playwright end-to-end tests** for all browser-facing features and server interface changes.
  When executing browser-hosted frontend checks, first try the MCP Playwright/browser tools. Fall back to repo-local Playwright commands only when the MCP surface is unavailable or cannot cover the scenario.
  Shared managed-viewer suites live under `viewer-api/viewer-api/frontend/dioxus/e2e/shared/`.
  Spec-viewer release E2E lives under `memory-viewers/spec-viewer/frontend/dioxus/`; run it with `npm run test:e2e:release`.
  Ticket-viewer release E2E lives under `memory-viewers/ticket-viewer/frontend/dioxus/`; run it with `npm run test:e2e:release`.
  Doc-viewer and log-viewer keep local Playwright wrappers under `tools/viewer/doc-viewer/e2e/` and `tools/viewer/log-viewer/e2e/`, importing shared suites from `memory-viewers/viewer-api`.
- For tracing-based tests, use: