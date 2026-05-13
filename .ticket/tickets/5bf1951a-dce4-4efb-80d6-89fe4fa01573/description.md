# ticket-vscode: Fix tree view state grouping

## Problem

The current `buildStateGroups()` in `ticketProvider.ts` extends each state folder
with **all transitive ancestors** of tickets in that state, regardless of the
ancestor's own state. This causes:

1. **Terminal tickets appear in active folders**: A `done` or `cancelled` parent
   shows up under `new`/`ready`/`in-implementation` folders if any of its
   dependency children are still in those states.
2. **Context pollution**: Users see irrelevant parent tickets mixed into folders
   where they don't belong, making the tree noisy and confusing.
3. **Hardcoded `STATE_ORDER`**: The extension hardcodes state names
   (`open`, `in-progress`, `review`, etc.) that don't match the actual
   schema states (`new`, `in-refinement`, `ready`, `in-implementation`,
   `in-review`, `in-validation`, `done`, `cancelled`). States should be
   loaded dynamically from the ticket server's `/api/schema` endpoint.

## Root Cause

In `buildStateGroups()` → `makeGroup()`:

```typescript
// Current: extends bucket with ALL ancestors regardless of state
const extendedIds = new Set(bucket.map(t => t.id));
for (const t of bucket) {
  for (const ancestorId of this._getAncestors(t.id)) {
    extendedIds.add(ancestorId);  // ← adds done/cancelled parents here
  }
}
```

## Required Behavior

Each state folder must show **only tickets whose own state matches the folder**.
Dependency hierarchy should still be displayed, but only when both parent and
child (direct or indirect) share the same state. Specifically:

1. **Strict state filtering**: A ticket appears in folder `S` only if
   `ticket.state === S`.
2. **Hierarchy within same state**: If ticket A depends on ticket B, and both
   are in state `S`, show B nested under A in the `S` folder.
3. **Transitive hierarchy**: If A→B→C all share state `S`, show the full
   chain A > B > C.
4. **Cross-state deps hidden**: If A (done) → B (in-implementation), A does
   NOT appear in the `in-implementation` folder. B appears at root level
   in `in-implementation`.
5. **Dynamic state list**: Fetch the schema from `GET /api/schema` and use
   the `states` array for ordering/grouping instead of the hardcoded
   `STATE_ORDER`.

## Acceptance Criteria

- [ ] AC1: Each state folder contains only tickets whose `state` field
      matches the folder label.
- [ ] AC2: Within a state folder, tickets with dependency relationships
      (direct or transitive, both in the same state) are displayed
      hierarchically — parent above child.
- [ ] AC3: Tickets in a state with no same-state parent appear at root
      level within that folder.
- [ ] AC4: The folder count label `"state (N)"` reflects the actual ticket
      count for that state.
- [ ] AC5: The hardcoded `STATE_ORDER` and `STATE_ICONS` are replaced by
      dynamically fetched schema states. Unknown states still appear
      alphabetically after known schema states.
- [ ] AC6: A new `fetchSchemas()` function is added to `api.ts` hitting
      `GET /api/schema?workspace=<ws>`.
- [ ] AC7: E2E tests using wdio-vscode-service validate tree structure
      against fixture ticket workspaces (see test plan below).

## Implementation Plan

### Step 1: Add schema fetch to `api.ts`

Add `fetchSchemas(baseUrl, workspace)` returning `TypeSchema[]` with states,
transitions, terminal_states. Cache in the provider on `load()`.

```typescript
export interface TypeSchema {
  type_id: string;
  states: string[];
  transitions: Array<{ from: string; to: string }>;
  required_states: string[];
  terminal_states: string[];
}

export async function fetchSchemas(
  baseUrl: string,
  workspace: string,
): Promise<TypeSchema[]> {
  const response = await apiFetch<{ types: TypeSchema[] }>(
    `${baseUrl}/api/schema?${new URLSearchParams({ workspace })}`,
  );
  return response.types;
}
```

### Step 2: Rewrite `buildStateGroups()` in `ticketProvider.ts`

Replace the ancestor-extension logic with strict same-state filtering:

```typescript
private buildStateGroups(): StateGroupItem[] {
  // 1. Group tickets by state
  const grouped = new Map<string, TicketSummary[]>();
  for (const ticket of this.tickets) {
    const s = ticket.state ?? 'unknown';
    let bucket = grouped.get(s);
    if (!bucket) { bucket = []; grouped.set(s, bucket); }
    bucket.push(ticket);
  }

  // 2. For each state, find root tickets (no same-state parent)
  const makeGroup = (s: string, bucket: TicketSummary[]): StateGroupItem => {
    const stateIds = new Set(bucket.map(t => t.id));
    const rootTickets: TicketSummary[] = [];
    for (const ticket of bucket) {
      const parents = this._parentOf.get(ticket.id) ?? [];
      // Root if no parent is also in this same state
      const hasSameStateParent = parents.some(pid => stateIds.has(pid));
      if (!hasSameStateParent) {
        rootTickets.push(ticket);
      }
    }
    return new StateGroupItem(s, bucket.length, rootTickets);
  };

  // 3. Order by schema states, then unknown alphabetically
  const result: StateGroupItem[] = [];
  const schemaStates = this._schemaStates ?? [];
  for (const s of schemaStates) {
    const bucket = grouped.get(s);
    if (bucket && bucket.length > 0) {
      result.push(makeGroup(s, bucket));
      grouped.delete(s);
    }
  }
  for (const [s, bucket] of [...grouped.entries()].sort(([a], [b]) => a.localeCompare(b))) {
    if (bucket.length > 0) {
      result.push(makeGroup(s, bucket));
    }
  }
  return result;
}
```

### Step 3: Filter `_getDependencyChildren` to same state

When expanding a ticket within a state folder, only show children that are
in the same state. Pass the state context through the tree path or a map.

```typescript
private _getDependencyChildren(parent: TicketItem): TicketItem[] {
  const depIds = this._depsOf.get(parent.ticket.id) ?? [];
  const parentState = parent.ticket.state;
  const children: TicketItem[] = [];
  for (const depId of depIds) {
    const ticket = this._ticketMap.get(depId);
    if (!ticket) continue;
    // Only show children that share the folder's state
    if (ticket.state === parentState) {
      children.push(this._makeTicketItem(ticket, parent.id ?? parent.ticket.id));
    }
  }
  return children;
}
```

### Step 4: Remove hardcoded `STATE_ORDER` and dynamic schema loading

- Remove `const STATE_ORDER` and `const STATE_ICONS` (or keep icons as a
  best-effort map with fallback `'tag'`).
- In `load()`, fetch schemas alongside tickets/edges:
  ```typescript
  const [tickets, edges, schemas] = await Promise.all([
    fetchAllTickets(...),
    fetchEdges(...).catch(() => []),
    fetchSchemas(...).catch(() => []),
  ]);
  this._schemaStates = schemas.flatMap(s => s.states);
  ```
- Add `private _schemaStates: string[] | undefined;`

### Step 5: E2E tests with wdio-vscode-service

#### Test infrastructure setup

1. Add `@vscode/vsce`, `@wdio/cli`, `wdio-vscode-service`, `@wdio/mocha-framework`,
   `@wdio/spec-reporter` as dev dependencies.
2. Create `wdio.conf.ts` configuration.
3. Create test fixtures under `tools/ticket-vscode/test-fixtures/`:

#### Static fixtures (`test-fixtures/workspace-a/.ticket/`)

Pre-seeded ticket workspace with known state distribution:

| Ticket | State | Depends On |
|--------|-------|-----------|
| parent-1 | done | child-a, child-b |
| child-a | in-implementation | — |
| child-b | in-implementation | — |
| standalone-1 | new | — |
| sibling-1 | in-review | sibling-2 |
| sibling-2 | in-review | — |
| chain-a | ready | chain-b |
| chain-b | ready | chain-c |
| chain-c | ready | — |

#### Programmatic fixtures (test setup)

Test helpers that create fixtures via the ticket CLI or HTTP API during setup,
then tear down after. Useful for testing edge cases and state transitions.

```typescript
async function createTestWorkspace(dir: string, tickets: TicketFixture[]) {
  // Use ticket CLI to create tickets + edges in a temp directory
}
```

#### Test cases

```
describe('State Folder Grouping', () => {
  it('each folder only contains tickets in that state')
  it('done parent does NOT appear in in-implementation folder')
  it('cancelled parent does NOT appear in child state folders')
  it('folder count matches actual ticket count for state')
})

describe('Dependency Hierarchy Within State', () => {
  it('sibling deps in same state show hierarchy')
  it('transitive chain in same state shows full hierarchy')
  it('cross-state children not nested under parent')
  it('ticket with no same-state parent appears at folder root')
})

describe('Dynamic State Loading', () => {
  it('state folders match schema states order')
  it('unknown states appear after schema states')
  it('icons use STATE_ICONS map with fallback')
})
```

## Risk Assessment

- **Low**: Core logic change is localized to `buildStateGroups()` and
  `_getDependencyChildren()`.
- **Medium**: E2E test infrastructure is new — framework setup may need
  iteration.
- **Low**: Schema fetch adds one extra HTTP call on load, cached per refresh.

## Files to Modify

- `tools/ticket-vscode/src/ticketProvider.ts` — rewrite grouping logic
- `tools/ticket-vscode/src/api.ts` — add `fetchSchemas()`
- `tools/ticket-vscode/package.json` — add test dev dependencies
- `tools/ticket-vscode/wdio.conf.ts` — new: WebdriverIO config
- `tools/ticket-vscode/test-fixtures/` — new: fixture workspaces
- `tools/ticket-vscode/test/` — new: e2e test files
