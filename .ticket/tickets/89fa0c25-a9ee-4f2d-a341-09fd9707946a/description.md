## Objective

Surface parts, refs, and frozen state in ticket-viewer so a human can read the full structured ticket, collapse or expand individual parts, see what is frozen, and follow typed references.

## Requirements

- A ticket page renders parts as distinct, individually collapsible sections in manifest order, not one concatenated body.
- The viewer always requests and renders the `full` projection; there is no profile switcher, segmented control, dropdown, or URL-synced profile state.
- Each part keeps its own collapse and expand state.
- Frozen parts are visually marked as frozen, with the state that froze them.
- An `amendment` part is rendered inline directly beneath the part it supersedes.
- Typed refs render as a list with kind, resolved title where resolvable, and note; `spec` refs deep-link to spec-viewer, `file` refs to the repo path, dangling refs are marked.
- Free-form parts render under `full` only, labelled as untyped attachments.
- The layout works at the browser widths used for manual validation.
- The release browser test covers the structured ticket rendering flow with screenshots for an expanded part, a collapsed part, and a frozen part with an amendment.

## Design

ticket-viewer is a managed viewer under memory-viewers/ticket-viewer with a Dioxus frontend; the read API it consumes lives in `memory-viewers/ticket-viewer/frontend/dioxus/src/api.rs` and `memory-viewers/ticket-viewer/frontend/dioxus/src/api/backend.rs`, while the main page and content components live in `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/page.rs`, `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/edit.rs`, and `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_detail/actions.rs`.

The UI always consumes the `full` projection, then uses structured part metadata to render per-part collapse state, frozen-state affordances, inline amendments, and typed references. There is no profile picker in the viewer; linkability comes from the ticket URL itself, while managed-viewer E2E suites live under `viewer-api/viewer-api/frontend/dioxus/e2e/shared/` and ticket-viewer release E2E runs from `memory-viewers/ticket-viewer/frontend/dioxus`.

## Implementation Steps

1. Extend `memory-viewers/ticket-viewer/frontend/dioxus/src/api.rs` and `memory-viewers/ticket-viewer/frontend/dioxus/src/api/backend.rs` so the viewer consumes the structured read response with part metadata, refs, frozen flags, and supersedes links.
2. Update `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/page.rs` to request the `full` projection and render each part as a collapsible section in manifest order with independent collapse state.
3. Update `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/edit.rs` and `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_detail/actions.rs` so the edit experience respects the current projection and does not collapse the structured view back into one blob.
4. Render frozen badges, inline amendment linkage, and typed ref rows in the ticket detail component, including deep links for `spec` refs and dangling-ref markings.
5. Remove the URL-synced profile control and replace it with local per-part collapse state that does not change the projection request.
6. Add or extend Playwright release E2E under `memory-viewers/ticket-viewer/frontend/dioxus/e2e/` to cover the expanded-part, collapsed-part, and frozen-with-amendment states, with screenshots captured for each.
7. Record the browser window size used for manual verification in the ticket's validation evidence when the release browser check is performed.

## Examples

A frozen `objective` shows a lock affordance reading "frozen at `planned`"; an `amendment` beneath it reads "supersedes objective".

## Acceptance Criteria

1. A ticket with every core part kind renders each as a separate collapsible section in manifest order.
2. The viewer requests the `full` projection, and there is no profile switcher, segmented control, dropdown, or URL-synced profile state in the UI.
3. Frozen parts are marked, and the marking names the freezing state.
4. Each part section can be independently collapsed or expanded without affecting the others.
5. An amendment renders inline directly beneath its superseded frozen part, in the same order as the projection contract.
6. A `spec` ref deep-links to spec-viewer; a dangling ref is visibly marked and does not break the page.
7. Free-form parts are absent from `summary`, `plan`, and `review`, and labelled as untyped under `full`.
8. Playwright release E2E covers criteria 1-7 with screenshots captured for an expanded part, a collapsed part, and a frozen part with an amendment.
9. Manually verified in an external fullscreen Chromium-family browser, with the window resolution recorded in this ticket's `validation` part.
10. A test-api validation execution is recorded for each acceptance criterion above, linked to this ticket id.

## Typed References

- spec: `ticket-api/entity/structured-ticket-entities` (24b3d22b)
- code: memory-viewers/ticket-viewer
- code: viewer-api/viewer-api/frontend/dioxus/e2e/shared