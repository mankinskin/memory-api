## Objective

Surface parts, refs, and frozen state in ticket-viewer so a human can read a structured ticket by part, see what is frozen, and follow typed references — with the same profile choices agents get.

## Requirements

- A ticket page renders parts as distinct, individually collapsible sections in manifest order, not one concatenated body.
- Frozen parts are visually marked as frozen, with the state that froze them.
- An `amendment` part is rendered in visible association with the part it supersedes.
- The four view profiles (`summary`, `plan`, `review`, `full`) are selectable in the UI and reflected in the URL so a view is linkable.
- Typed refs render as a list with kind, resolved title where resolvable, and note; `spec` refs deep-link to spec-viewer, `file` refs to the repo path, dangling refs are marked.
- Free-form parts render under `full` only, labelled as untyped attachments.
- The layout works at the browser widths used for manual validation.
- The release browser test covers the structured ticket rendering flow with screenshots for each profile and the frozen/amendment state.

## Design

ticket-viewer is a managed viewer under memory-viewers/ticket-viewer with a Dioxus frontend; the read API it consumes lives in `memory-viewers/ticket-viewer/frontend/dioxus/src/api.rs` and `memory-viewers/ticket-viewer/frontend/dioxus/src/api/backend.rs`, while the main page and content components live in `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/page.rs`, `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/edit.rs`, and `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_detail/actions.rs`.

The UI follows the same projection contract as 4c7b884e: it renders the selected profile or explicit parts, preserves manifest order, and uses the structured part metadata for frozen-state and amendment affordances. Shared managed-viewer E2E suites live under `viewer-api/viewer-api/frontend/dioxus/e2e/shared/`; ticket-viewer release E2E runs from `memory-viewers/ticket-viewer/frontend/dioxus`.

## Implementation Steps

1. Extend `memory-viewers/ticket-viewer/frontend/dioxus/src/api.rs` and `memory-viewers/ticket-viewer/frontend/dioxus/src/api/backend.rs` so the viewer consumes the structured read response with part metadata, refs, frozen flags, and supersedes links.
2. Update `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/page.rs` to render each part as a collapsible section in manifest order and to switch between `summary`, `plan`, `review`, `full`, or explicit part selections.
3. Update `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/edit.rs` and `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_detail/actions.rs` so the edit experience respects the current projection and does not collapse the structured view back into one blob.
4. Render frozen badges, amendment linkage, and typed ref rows in the ticket detail component, including deep links for `spec` refs and dangling-ref markings.
5. Thread the selected profile into the URL and back out again so the page is linkable and reload-safe.
6. Add or extend Playwright release E2E under `memory-viewers/ticket-viewer/frontend/dioxus/e2e/` to cover every profile plus the frozen/amendment state, with screenshots captured for each key view.
7. Record the browser window size used for manual verification in the ticket's validation evidence when the release browser check is performed.

## Examples

URL shape: `http://localhost:3002/workspace/default/ticket/{id}?view=plan`

A frozen `objective` shows a lock affordance reading "frozen at `planned`"; an `amendment` beneath it reads "supersedes objective".

## Acceptance Criteria

1. A ticket with every core part kind renders each as a separate collapsible section in manifest order.
2. Frozen parts are marked, and the marking names the freezing state.
3. An amendment renders in visible association with its superseded part.
4. Selecting each of the four profiles changes the rendered part set to match the projection contract, and the URL reflects the selection.
5. A `spec` ref deep-links to spec-viewer; a dangling ref is visibly marked and does not break the page.
6. Free-form parts are absent from `summary`, `plan`, and `review`, and labelled as untyped under `full`.
7. Playwright release E2E covers criteria 1-6 with screenshots captured for each profile and for the frozen/amendment state.
8. Manually verified in an external fullscreen Chromium-family browser, with the window resolution recorded in this ticket's `validation` part.
9. A test-api validation execution is recorded for each acceptance criterion above, linked to this ticket id.

## Typed References

- spec: `ticket-api/entity/structured-ticket-entities` (24b3d22b)
- code: memory-viewers/ticket-viewer
- code: viewer-api/viewer-api/frontend/dioxus/e2e/shared