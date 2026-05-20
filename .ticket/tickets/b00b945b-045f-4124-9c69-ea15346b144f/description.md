# [ticket-viewer] Fix list-driven content panel selection sync

## Problem
Opening a different ticket from the sidebar could leave the content panel showing the previous ticket body because the content component kept its state across list-driven selection changes.

## Implementation
- Key the split-view content panel by active ticket reference plus active asset path so TicketContent remounts when sidebar selection changes.
- Add a release Playwright regression that opens a parent ticket, then a child ticket, and asserts the description body and URL hash switch to the newly selected ticket.

## Validation
- cargo check --manifest-path memory-viewers/ticket-viewer/frontend/dioxus/Cargo.toml --target wasm32-unknown-unknown
- viewer-ctl prepare ticket-viewer && cargo build -p ticket-viewer --release && cd memory-viewers/ticket-viewer/frontend/dioxus && npm run test:e2e:release -- mixed-workspace-root-route.spec.ts -g "swaps content panel body when clicking different ticket rows"
- Managed viewer visual verification in headed Edge at 1440x900: clicked two sidebar ticket rows and confirmed the content panel rendered two different description bodies.

## Docs
- Updated memory-viewers/.spec/specs/33e731c2-a0cf-41f7-bd7c-2df6c4545bf3/body.md with the sidebar selection acceptance behavior and validation evidence.
