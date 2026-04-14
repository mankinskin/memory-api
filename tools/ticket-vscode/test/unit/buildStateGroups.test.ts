/* eslint-disable @typescript-eslint/no-explicit-any */
/**
 * Unit tests for TicketTreeProvider.buildStateGroups() logic.
 *
 * These tests validate the strict same-state grouping behavior introduced
 * to fix: cancelled/done tickets appearing in active-state folders when they
 * are ancestors of tickets in that state.
 *
 * Regression test for:
 *   ticket 5bf1951a — Fix tree view state grouping
 *   Bug: 48ea4df8 (cancelled) appeared in "new" folder because it depends_on
 *        ee43f72e (new), and the old ancestor-promotion code added it there.
 */

import { TicketTreeProvider, StateGroupItem, TicketItem } from '../../src/ticketProvider';
import type { TicketSummary, EdgeRecord } from '../../src/api';

// Mock the API module so we can inject controlled test data.
jest.mock('../../src/api', () => ({
  fetchAllTickets: jest.fn(),
  fetchEdges: jest.fn(),
  fetchSchemas: jest.fn(),
  fetchTicketDescription: jest.fn(),
}));
import * as api from '../../src/api';

// Mock fs so _getTicketFolderChildren always returns empty (no disk access).
jest.mock('node:fs', () => ({
  readdirSync: jest.fn(() => []),
}));

const SCHEMA_STATES = ['new', 'ready', 'in-implementation', 'in-review', 'done', 'cancelled'];

function makeTicket(
  id: string,
  state: string,
  title = `Ticket ${id.slice(0, 8)}`,
): TicketSummary {
  return {
    id,
    type: 'tracker-improvement',
    title,
    state,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    fields: {},
  };
}

function makeEdge(from: string, to: string): EdgeRecord {
  return { from, to, kind: 'depends_on' };
}

/**
 * Create the provider and wait for the initial load to complete.
 * Injects controlled tickets, edges, and schema via API mocks.
 */
async function buildProvider(
  tickets: TicketSummary[],
  edges: EdgeRecord[],
): Promise<TicketTreeProvider> {
  const mockApi = api as jest.Mocked<typeof api>;
  mockApi.fetchAllTickets.mockResolvedValue(tickets);
  mockApi.fetchEdges.mockResolvedValue(edges);
  mockApi.fetchSchemas.mockResolvedValue([
    {
      type_id: 'tracker-improvement',
      states: SCHEMA_STATES,
      transitions: [],
      required_states: ['in-review'],
      terminal_states: ['done', 'cancelled'],
    },
  ]);

  const provider = new TicketTreeProvider(
    'http://localhost:3002',
    'default',
    0, // no auto-refresh
  );

  // Wait for the async load() to complete by polling the idle state.
  // load() fires onDidChangeTreeData twice: at start (loading) and on finish.
  await new Promise<void>(resolve => {
    const sub = provider.onDidChangeTreeData(() => {
      // The second fire happens when loading is done (idle or error).
      if ((provider as any).state !== 'loading') {
        sub.dispose();
        resolve();
      }
    });
  });

  return provider;
}

/** Get all root-level state group items. */
function getRootGroups(provider: TicketTreeProvider): StateGroupItem[] {
  return provider.getChildren(undefined) as StateGroupItem[];
}

/** Get the state group for the given state, or null. */
function getGroup(provider: TicketTreeProvider, state: string): StateGroupItem | null {
  return getRootGroups(provider).find(g => g.state === state) ?? null;
}

/** Get the displayed ticket items inside a state group. */
function getGroupItems(provider: TicketTreeProvider, group: StateGroupItem): TicketItem[] {
  return provider.getChildren(group) as TicketItem[];
}

/** Collect all ticket IDs visible recursively in a state folder (BFS, depth-limited). */
function collectAllVisibleIds(
  provider: TicketTreeProvider,
  group: StateGroupItem,
  maxDepth = 5,
): Set<string> {
  const result = new Set<string>();
  const queue: Array<{ item: TicketItem; depth: number }> = getGroupItems(provider, group).map(
    item => ({ item, depth: 0 }),
  );
  while (queue.length > 0) {
    const { item, depth } = queue.shift()!;
    result.add(item.ticket.id);
    if (depth < maxDepth) {
      const children = provider.getChildren(item) as TicketItem[];
      for (const child of children) {
        if (child instanceof TicketItem) {
          queue.push({ item: child, depth: depth + 1 });
        }
      }
    }
  }
  return result;
}

// ── IDs for the real-world regression scenario ───────────────────────────────
const CANCELLED_PARENT_ID = '48ea4df8-25f5-46ce-b2cc-ff00d32ddd47';
const NEW_CHILD_ID = 'ee43f72e-53ef-4937-8216-92e17f185d85';

describe('TicketTreeProvider — state folder grouping', () => {
  afterEach(() => {
    jest.clearAllMocks();
  });

  // ── Regression: AC1 — strict state filtering ─────────────────────────────

  describe('AC1 — each folder only contains tickets whose state matches the folder', () => {
    test('cancelled parent does NOT appear in "new" folder (real IDs regression)', async () => {
      /**
       * REGRESSION TEST for ticket 5bf1951a.
       *
       * Setup:
       *   48ea4df8 (cancelled) depends_on ee43f72e (new)
       *
       * Old behaviour: 48ea4df8 appeared in the "new" folder via ancestor promotion.
       * Expected:      48ea4df8 must ONLY appear in "cancelled" folder.
       */
      const tickets = [
        makeTicket(CANCELLED_PARENT_ID, 'cancelled', '[bootstrap] run one-week dogfood trial'),
        makeTicket(NEW_CHILD_ID, 'new', '[bootstrap] write test fixtures'),
      ];
      const edges = [makeEdge(CANCELLED_PARENT_ID, NEW_CHILD_ID)];

      const provider = await buildProvider(tickets, edges);

      // 48ea4df8 must NOT appear in "new" folder (root or nested)
      const newGroup = getGroup(provider, 'new');
      expect(newGroup).not.toBeNull();
      const visibleInNew = newGroup ? collectAllVisibleIds(provider, newGroup) : new Set();
      expect(visibleInNew.has(CANCELLED_PARENT_ID)).toBe(false);

      // 48ea4df8 MUST appear in "cancelled" folder
      const cancelledGroup = getGroup(provider, 'cancelled');
      expect(cancelledGroup).not.toBeNull();
      const visibleInCancelled = cancelledGroup ? collectAllVisibleIds(provider, cancelledGroup) : new Set();
      expect(visibleInCancelled.has(CANCELLED_PARENT_ID)).toBe(true);
    });

    test('done parent does NOT appear in "in-implementation" folder', async () => {
      const DONE = 'd0000000-0000-0000-0000-000000000001';
      const IMPL = 'i0000000-0000-0000-0000-000000000002';
      const tickets = [
        makeTicket(DONE, 'done', 'Parent epic (done)'),
        makeTicket(IMPL, 'in-implementation', 'Child work item'),
      ];
      const edges = [makeEdge(DONE, IMPL)];
      const provider = await buildProvider(tickets, edges);

      const implGroup = getGroup(provider, 'in-implementation');
      expect(implGroup).not.toBeNull();
      const visible = implGroup ? collectAllVisibleIds(provider, implGroup) : new Set();
      expect(visible.has(DONE)).toBe(false);
      expect(visible.has(IMPL)).toBe(true);
    });

    test('folder count matches actual ticket count for state (AC4)', async () => {
      const tickets = [
        makeTicket('a0000000-0000-0000-0000-000000000001', 'new'),
        makeTicket('a0000000-0000-0000-0000-000000000002', 'new'),
        makeTicket('a0000000-0000-0000-0000-000000000003', 'cancelled'),
      ];
      const provider = await buildProvider(tickets, []);

      const newGroup = getGroup(provider, 'new');
      expect(newGroup?.totalCount).toBe(2); // only 2 "new" tickets

      const cancelledGroup = getGroup(provider, 'cancelled');
      expect(cancelledGroup?.totalCount).toBe(1);
    });
  });

  // ── AC2/AC3 — hierarchy within same state ─────────────────────────────────

  describe('AC2/AC3 — hierarchy within same state', () => {
    test('sibling deps both in same state show hierarchically', async () => {
      const PARENT = 'b0000000-0000-0000-0000-000000000001';
      const CHILD = 'b0000000-0000-0000-0000-000000000002';
      const tickets = [
        makeTicket(PARENT, 'in-review', 'Parent (in-review)'),
        makeTicket(CHILD, 'in-review', 'Child (in-review)'),
      ];
      const edges = [makeEdge(PARENT, CHILD)];
      const provider = await buildProvider(tickets, edges);

      const group = getGroup(provider, 'in-review');
      expect(group).not.toBeNull();

      const rootItems = group ? getGroupItems(provider, group) : [];
      // Only parent is root; child is not (it has a same-state parent)
      expect(rootItems.length).toBe(1);
      expect(rootItems[0].ticket.id).toBe(PARENT);

      // Expanding the parent shows the child
      const childItems = provider.getChildren(rootItems[0]) as TicketItem[];
      const depChildren = childItems.filter(i => i instanceof TicketItem);
      expect(depChildren.some(i => i.ticket.id === CHILD)).toBe(true);
    });

    test('transitive chain A→B→C all in same state shows full hierarchy', async () => {
      const A = 'c0000000-0000-0000-0000-000000000001';
      const B = 'c0000000-0000-0000-0000-000000000002';
      const C = 'c0000000-0000-0000-0000-000000000003';
      const tickets = [
        makeTicket(A, 'ready', 'A'),
        makeTicket(B, 'ready', 'B'),
        makeTicket(C, 'ready', 'C'),
      ];
      const edges = [makeEdge(A, B), makeEdge(B, C)];
      const provider = await buildProvider(tickets, edges);

      const group = getGroup(provider, 'ready');
      expect(group?.totalCount).toBe(3);

      const rootItems = group ? getGroupItems(provider, group) : [];
      // Only A is root
      expect(rootItems.length).toBe(1);
      expect(rootItems[0].ticket.id).toBe(A);

      // A → B
      const aChildren = (provider.getChildren(rootItems[0]) as TicketItem[]).filter(
        i => i instanceof TicketItem,
      );
      expect(aChildren.length).toBe(1);
      expect(aChildren[0].ticket.id).toBe(B);

      // B → C
      const bChildren = (provider.getChildren(aChildren[0]) as TicketItem[]).filter(
        i => i instanceof TicketItem,
      );
      expect(bChildren.length).toBe(1);
      expect(bChildren[0].ticket.id).toBe(C);
    });

    test('ticket with no same-state parent appears at folder root (AC3)', async () => {
      const LONE = 'e0000000-0000-0000-0000-000000000001';
      const tickets = [makeTicket(LONE, 'new', 'Standalone ticket')];
      const provider = await buildProvider(tickets, []);

      const group = getGroup(provider, 'new');
      const rootItems = group ? getGroupItems(provider, group) : [];
      expect(rootItems.some(i => i.ticket.id === LONE)).toBe(true);
    });

    test('cross-state dep child NOT shown under parent in different state folder', async () => {
      const DONE = 'f0000000-0000-0000-0000-000000000001';
      const NEW = 'f0000000-0000-0000-0000-000000000002';
      const tickets = [
        makeTicket(DONE, 'done', 'Done parent'),
        makeTicket(NEW, 'new', 'New child'),
      ];
      const edges = [makeEdge(DONE, NEW)];
      const provider = await buildProvider(tickets, edges);

      // Expanding DONE in "done" folder should NOT show NEW as a child
      const doneGroup = getGroup(provider, 'done');
      const doneRoots = doneGroup ? getGroupItems(provider, doneGroup) : [];
      const doneParent = doneRoots.find(i => i.ticket.id === DONE);
      expect(doneParent).toBeDefined();

      if (doneParent) {
        const children = (provider.getChildren(doneParent) as TicketItem[]).filter(
          i => i instanceof TicketItem,
        );
        expect(children.some(i => i.ticket.id === NEW)).toBe(false);
      }
    });
  });

  // ── AC5 — dynamic state ordering ─────────────────────────────────────────

  describe('AC5 — state folders ordered by schema states', () => {
    test('schema states appear before unknown states', async () => {
      const tickets = [
        makeTicket('00000000-0000-0000-0000-000000000001', 'new'),
        makeTicket('00000000-0000-0000-0000-000000000002', 'zz-custom-state'),
      ];
      const provider = await buildProvider(tickets, []);

      const groups = getRootGroups(provider);
      const states = groups.filter(g => g instanceof StateGroupItem).map(g => g.state);

      const newIdx = states.indexOf('new');
      const customIdx = states.indexOf('zz-custom-state');

      expect(newIdx).toBeGreaterThanOrEqual(0);
      expect(customIdx).toBeGreaterThanOrEqual(0);
      // 'new' is a schema state → should appear before the unknown custom state
      expect(newIdx).toBeLessThan(customIdx);
    });
  });
});
