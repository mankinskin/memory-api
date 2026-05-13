# Show ticket description in hover tooltip

## Problem

Currently, hovering over a ticket in the tree view shows only basic metadata: title, ID, state, and type. The ticket description (which contains context, acceptance criteria, and implementation details) is not visible without navigating to the full ticket view. Users frequently need to glance at a description to decide which ticket to work on.

## Solution

Fetch the ticket description via the `/api/tickets/{id}/description` endpoint and display it as markdown in the VS Code hover tooltip. Show the full description since VS Code tooltips support scrolling for long content rendered as MarkdownString.

### Implementation Plan

1. **Add `fetchTicketDescription()` to `api.ts`**:
   ```typescript
   export interface TicketDescriptionResponse {
     request_id: string;
     workspace: string;
     id: string;
     description: string | null;
   }

   export async function fetchTicketDescription(
     baseUrl: string,
     workspace: string,
     ticketId: string,
   ): Promise<string | null> {
     const params = new URLSearchParams({ workspace });
     const data = await apiFetch<TicketDescriptionResponse>(
       `${baseUrl}/api/tickets/${encodeURIComponent(ticketId)}/description?${params}`
     );
     return data.description;
   }
   ```

2. **Implement `resolveTreeItem()` in `TicketTreeProvider`**:
   - VS Code's `TreeDataProvider` supports lazy tooltip resolution via `resolveTreeItem()`.
   - Only `TicketItem` nodes need resolution; `StateGroupItem` and `InfoItem` return as-is.
   - On resolve, call `fetchTicketDescription()` and set the tooltip to a `MarkdownString` with title, metadata, and the full description body.
   - Cache the description in a `Map<string, string | null>` keyed by ticket ID to avoid re-fetching on every hover. Clear cache on refresh.

3. **Update `TicketItem` constructor**:
   - Set initial tooltip to basic metadata (current behavior) — this is shown while the description loads.
   - The `resolveTreeItem()` call will enhance it asynchronously.

4. **Pass server URL and workspace to `TicketTreeProvider`**:
   - Already available as `_baseUrl` and `_workspace`. The `resolveTreeItem` method can access them directly.

### Files to Modify

- `tools/ticket-vscode/src/api.ts` — add `fetchTicketDescription()` function and response type
- `tools/ticket-vscode/src/ticketProvider.ts` — implement `resolveTreeItem()`, add description cache

## Acceptance Criteria

- [ ] Hovering over a ticket in the tree view shows a markdown tooltip with the full description
- [ ] If description is `null` or fetch fails, the tooltip falls back to the existing basic metadata
- [ ] Descriptions are cached per ticket ID and cleared on tree refresh
- [ ] The tooltip includes title, ID, state, type, and description body formatted as markdown
- [ ] No visible lag when hovering — initial tooltip shows immediately, description enhances it asynchronously
- [ ] Extension compiles without errors (`npm run compile`)

## Risk

Low — `resolveTreeItem` is the standard lazy-loading pattern for VS Code tree views. The new API call is read-only. Cache prevents excessive HTTP requests.