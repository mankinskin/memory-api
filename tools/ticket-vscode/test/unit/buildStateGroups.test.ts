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
 *   Bug: 48ea4df8 (cancelled) appeared in "open" folder because it depends_on
 *        ee43f72e (new), and the old ancestor-promotion code added it there.
 */

import { TicketTreeProvider, StateGroupItem, TicketItem } from '../../src/ticketProvider';
import { FilterControlItem, InfoItem } from '../../src/ticketTreeItems';
import type { TicketSummary, EdgeRecord } from '../../src/api';
import type { CoreApi } from '../../src/coreLoader';

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

const SCHEMA_STATES = ['open', 'planned', 'in-implementation', 'in-review', 'done', 'cancelled'];

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

function makeCoreMock(): CoreApi {
  function matchesTicket(ticket: TicketSummary, stateFilter: string, query: string): boolean {
    const state = stateFilter.trim();
    if (state && ticket.state !== state) {
      return false;
    }

    const needle = query.trim().toLowerCase();
    if (!needle) {
      return true;
    }

    return (ticket.title ?? '').toLowerCase().includes(needle)
      || ticket.id.toLowerCase().includes(needle);
  }

  return {
    core_version: () => 'test-core',
    ticket_matches: matchesTicket,
    build_dependency_maps(tickets, edges) {
      const ticketIds = new Set(tickets.map(ticket => ticket.id));
      const depsOf = new Map<string, string[]>();
      const parentOf = new Map<string, string[]>();

      for (const edge of edges) {
        if (!ticketIds.has(edge.from) || !ticketIds.has(edge.to)) {
          continue;
        }

        const deps = depsOf.get(edge.from) ?? [];
        deps.push(edge.to);
        depsOf.set(edge.from, deps);

        const parents = parentOf.get(edge.to) ?? [];
        parents.push(edge.from);
        parentOf.set(edge.to, parents);
      }

      return { depsOf, parentOf };
    },
    build_state_groups(tickets, edges, stateOrder, query) {
      const visibleTickets = tickets.filter(ticket => matchesTicket(ticket, '', query));
      const ticketIds = new Set(visibleTickets.map(ticket => ticket.id));
      const parentOf = new Map<string, string[]>();

      for (const edge of edges) {
        if (!ticketIds.has(edge.from) || !ticketIds.has(edge.to)) {
          continue;
        }

        const parents = parentOf.get(edge.to) ?? [];
        parents.push(edge.from);
        parentOf.set(edge.to, parents);
      }

      const grouped = new Map<string, TicketSummary[]>();
      for (const ticket of visibleTickets) {
        const state = ticket.state ?? 'unknown';
        const bucket = grouped.get(state) ?? [];
        bucket.push(ticket);
        grouped.set(state, bucket);
      }

      const makeGroup = (state: string, bucket: TicketSummary[]) => {
        const stateIds = new Set(bucket.map(ticket => ticket.id));
        const rootIds = bucket
          .filter(ticket => !(parentOf.get(ticket.id) ?? []).some(parentId => stateIds.has(parentId)))
          .map(ticket => ticket.id);
        return { state, total: bucket.length, rootIds };
      };

      const result: Array<{ state: string; total: number; rootIds: string[] }> = [];
      for (const state of stateOrder) {
        const bucket = grouped.get(state);
        if (bucket && bucket.length > 0) {
          result.push(makeGroup(state, bucket));
          grouped.delete(state);
        }
      }
      for (const [state, bucket] of [...grouped.entries()].sort(([a], [b]) => a.localeCompare(b))) {
        if (bucket.length > 0) {
          result.push(makeGroup(state, bucket));
        }
      }
      return result;
    },
    supports_server_control: () => true,
    supports_browser_bridge: () => true,
    supports_file_browsing: () => true,
    ticket_viewer_url: (baseUrl, workspace, ticketId) => `${baseUrl}/workspace/${encodeURIComponent(workspace)}/ticket/${encodeURIComponent(ticketId)}`,
    ticket_display_label: (id, title) => title ?? id,
  };
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

  const core = makeCoreMock();

  const provider = new TicketTreeProvider(
    'http://localhost:3002',
    'default',
    0, // no auto-refresh
    undefined,
    undefined,
    undefined,
    core,
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

async function waitForProviderReload(
  provider: TicketTreeProvider,
  action: () => void,
): Promise<void> {
  await new Promise<void>(resolve => {
    const sub = provider.onDidChangeTreeData(() => {
      if ((provider as any).state !== 'loading') {
        sub.dispose();
        resolve();
      }
    });
    action();
  });
}

/** Get all root-level state group items. */
function getRootGroups(provider: TicketTreeProvider): StateGroupItem[] {
  return (provider.getChildren(undefined) as Array<StateGroupItem | FilterControlItem>)
    .filter((item): item is StateGroupItem => item instanceof StateGroupItem);
}

function getRootControls(provider: TicketTreeProvider): FilterControlItem[] {
  return (provider.getChildren(undefined) as Array<StateGroupItem | FilterControlItem>)
    .filter((item): item is FilterControlItem => item instanceof FilterControlItem);
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
    test('cancelled parent does NOT appear in "open" folder (real IDs regression)', async () => {
      /**
       * REGRESSION TEST for ticket 5bf1951a.
       *
       * Setup:
       *   48ea4df8 (cancelled) depends_on ee43f72e (new)
       *
       * Old behaviour: 48ea4df8 appeared in the "open" folder via ancestor promotion.
       * Expected:      48ea4df8 must ONLY appear in "cancelled" folder.
       */
      const tickets = [
        makeTicket(CANCELLED_PARENT_ID, 'cancelled', '[bootstrap] run one-week dogfood trial'),
        makeTicket(NEW_CHILD_ID, 'open', '[bootstrap] write test fixtures'),
      ];
      const edges = [makeEdge(CANCELLED_PARENT_ID, NEW_CHILD_ID)];

      const provider = await buildProvider(tickets, edges);

      // 48ea4df8 must NOT appear in "open" folder (root or nested)
      const newGroup = getGroup(provider, 'open');
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
        makeTicket('a0000000-0000-0000-0000-000000000001', 'open'),
        makeTicket('a0000000-0000-0000-0000-000000000002', 'open'),
        makeTicket('a0000000-0000-0000-0000-000000000003', 'cancelled'),
      ];
      const provider = await buildProvider(tickets, []);

      const newGroup = getGroup(provider, 'open');
      expect(newGroup?.totalCount).toBe(2); // only 2 "open" tickets

      const cancelledGroup = getGroup(provider, 'cancelled');
      expect(cancelledGroup?.totalCount).toBe(1);
    });
  });

  test('recovers from an initial ticket fetch failure by rebinding and retrying once', async () => {
    const tickets = [
      makeTicket('a0000000-0000-0000-0000-000000000001', 'open', 'Recovered ticket'),
    ];
    const mockApi = api as jest.Mocked<typeof api>;
    const recovery = jest.fn().mockResolvedValue({
      baseUrl: 'http://localhost:55838',
      workspace: 'shared--abc123',
      ticketsDirUri: { fsPath: 'C:/tickets' },
    });

    mockApi.fetchAllTickets
      .mockRejectedValueOnce(new Error('connect ECONNREFUSED 127.0.0.1:3002'))
      .mockResolvedValueOnce(tickets);
    mockApi.fetchEdges.mockResolvedValue([]);
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
      0,
      undefined,
      recovery,
      undefined,
      makeCoreMock(),
    );

    await new Promise<void>(resolve => {
      const sub = provider.onDidChangeTreeData(() => {
        if ((provider as any).state !== 'loading') {
          sub.dispose();
          resolve();
        }
      });
    });

    expect(recovery).toHaveBeenCalledTimes(1);
    expect(mockApi.fetchAllTickets).toHaveBeenNthCalledWith(1, 'http://localhost:3002', 'default', {});
    expect(mockApi.fetchAllTickets).toHaveBeenNthCalledWith(2, 'http://localhost:55838', 'shared--abc123', {});
    expect(provider.allTickets).toHaveLength(1);
  });

  test('surfaces request URL, workspace, filters, and response details in the error state', async () => {
    const mockApi = api as jest.Mocked<typeof api>;
    mockApi.fetchAllTickets.mockRejectedValue(
      {
        name: 'ApiRequestError',
        message: 'List tickets failed (GET http://localhost:3002/api/tickets?workspace=default&limit=500&state=new) -> HTTP 404: {"code":"not_found","message":"workspace not found"}',
        operation: 'List tickets',
        url: 'http://localhost:3002/api/tickets?workspace=default&limit=500&state=new',
        method: 'GET',
        status: 404,
        responseBody: '{"code":"not_found","message":"workspace not found"}',
      },
    );
    mockApi.fetchEdges.mockResolvedValue([]);
    mockApi.fetchSchemas.mockResolvedValue([]);

    const provider = new TicketTreeProvider(
      'http://localhost:3002',
      'default',
      0,
      undefined,
      undefined,
      undefined,
      makeCoreMock(),
    );

    await waitForProviderReload(provider, () => {
      provider.setStateFilter('open');
    });

    const infoItems = provider.getChildren(undefined)
      .filter((item): item is InfoItem => item instanceof InfoItem);
    expect(infoItems).toHaveLength(1);
    const [errorItem] = infoItems;
    const tooltip = String(errorItem.tooltip);
    expect(errorItem.label).toBe('Request failed (HTTP 404): List tickets');
    expect(tooltip).toContain('Server URL: http://localhost:3002');
    expect(tooltip).toContain('Workspace: default');
    expect(tooltip).toContain('Filters: state=new');
    expect(tooltip).toContain('Operation: List tickets');
    expect(tooltip).toContain('Request: GET http://localhost:3002/api/tickets?workspace=default&limit=500&state=new');
    expect(tooltip).toContain('HTTP status: 404');
    expect(tooltip).toContain('workspace not found');
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
        makeTicket(A, 'planned', 'A'),
        makeTicket(B, 'planned', 'B'),
        makeTicket(C, 'planned', 'C'),
      ];
      const edges = [makeEdge(A, B), makeEdge(B, C)];
      const provider = await buildProvider(tickets, edges);

      const group = getGroup(provider, 'planned');
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
      const tickets = [makeTicket(LONE, 'open', 'Standalone ticket')];
      const provider = await buildProvider(tickets, []);

      const group = getGroup(provider, 'open');
      const rootItems = group ? getGroupItems(provider, group) : [];
      expect(rootItems.some(i => i.ticket.id === LONE)).toBe(true);
    });

    test('cross-state dep child NOT shown under parent in different state folder', async () => {
      const DONE = 'f0000000-0000-0000-0000-000000000001';
      const NEW = 'f0000000-0000-0000-0000-000000000002';
      const tickets = [
        makeTicket(DONE, 'done', 'Done parent'),
        makeTicket(NEW, 'open', 'New child'),
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
        makeTicket('00000000-0000-0000-0000-000000000001', 'open'),
        makeTicket('00000000-0000-0000-0000-000000000002', 'zz-custom-state'),
      ];
      const provider = await buildProvider(tickets, []);

      const groups = getRootGroups(provider);
      const states = groups.filter(g => g instanceof StateGroupItem).map(g => g.state);

      const newIdx = states.indexOf('open');
      const customIdx = states.indexOf('zz-custom-state');

      expect(newIdx).toBeGreaterThanOrEqual(0);
      expect(customIdx).toBeGreaterThanOrEqual(0);
      // 'open' is a schema state → should appear before the unknown custom state
      expect(newIdx).toBeLessThan(customIdx);
    });
  });

  describe('filter-backed reloads', () => {
    test('shows visible root controls for search and state filter', async () => {
      const provider = await buildProvider([makeTicket('30000000-0000-0000-0000-000000000001', 'open')], []);

      const controls = getRootControls(provider);
      expect(controls.map(control => control.label)).toEqual(['Search Tickets', 'Filter By State']);
      expect(controls.map(control => control.description)).toEqual(['None', 'All states']);
    });

    test('setLocalSearch filters in-memory tickets without calling fetchAllTickets', async () => {
      const tickets = [
        makeTicket('40000000-0000-0000-0000-000000000001', 'open', 'Fix authentication bug'),
        makeTicket('40000000-0000-0000-0000-000000000002', 'open', 'Add export button'),
        makeTicket('40000000-0000-0000-0000-000000000003', 'planned', 'Improve logging'),
      ];
      const provider = await buildProvider(tickets, []);
      const mockApi = api as jest.Mocked<typeof api>;

      const callsBefore = mockApi.fetchAllTickets.mock.calls.length;
      provider.setLocalSearch('auth');

      // No extra server call
      expect(mockApi.fetchAllTickets.mock.calls.length).toBe(callsBefore);

      // Only the matching 'open' ticket should be visible
      const groups = getRootGroups(provider);
      expect(groups.map(g => g.state)).toEqual(['open']);
      const items = provider.getChildren(groups[0]) as TicketItem[];
      expect(items.length).toBe(1);
      expect(items[0].ticket.title).toBe('Fix authentication bug');
    });

    test('search control description updates live as setLocalSearch is called', async () => {
      const provider = await buildProvider([makeTicket('50000000-0000-0000-0000-000000000001', 'open')], []);

      provider.setLocalSearch('foo');
      expect(getRootControls(provider)[0].description).toBe('foo');

      provider.setLocalSearch('');
      expect(getRootControls(provider)[0].description).toBe('None');
    });

    test('forwards active search and state filters to fetchAllTickets', async () => {
      const READY = '10000000-0000-0000-0000-000000000001';
      const provider = await buildProvider([makeTicket(READY, 'planned', 'Needle ticket')], []);
      const mockApi = api as jest.Mocked<typeof api>;

      mockApi.fetchAllTickets.mockResolvedValueOnce([
        makeTicket(READY, 'planned', 'Needle ticket'),
      ]);
      await waitForProviderReload(provider, () => provider.setSearchQuery('needle'));

      expect(mockApi.fetchAllTickets).toHaveBeenLastCalledWith(
        'http://localhost:3002',
        'default',
        { query: 'needle' },
      );
      expect(provider.filterSummary).toContain('needle');

      mockApi.fetchAllTickets.mockResolvedValueOnce([
        makeTicket(READY, 'planned', 'Needle ticket'),
      ]);
      await waitForProviderReload(provider, () => provider.setStateFilter('planned'));

      expect(mockApi.fetchAllTickets).toHaveBeenLastCalledWith(
        'http://localhost:3002',
        'default',
        { query: 'needle', state: 'planned' },
      );
      expect(getRootControls(provider).map(control => control.description)).toEqual(['needle', 'planned']);
      expect(getRootGroups(provider).map(group => group.state)).toEqual(['planned']);
    });

    test('clearFilters restores the unfiltered ticket groups', async () => {
      const NEW = '20000000-0000-0000-0000-000000000001';
      const READY = '20000000-0000-0000-0000-000000000002';
      const initialTickets = [
        makeTicket(NEW, 'open', 'New ticket'),
        makeTicket(READY, 'planned', 'Ready ticket'),
      ];
      const provider = await buildProvider(initialTickets, []);
      const mockApi = api as jest.Mocked<typeof api>;

      mockApi.fetchAllTickets.mockResolvedValueOnce([
        makeTicket(READY, 'planned', 'Ready ticket'),
      ]);
      await waitForProviderReload(provider, () => provider.setStateFilter('planned'));
      expect(getRootGroups(provider).map(group => group.state)).toEqual(['planned']);

      mockApi.fetchAllTickets.mockResolvedValueOnce(initialTickets);
      await waitForProviderReload(provider, () => provider.clearFilters());

      expect(mockApi.fetchAllTickets).toHaveBeenLastCalledWith(
        'http://localhost:3002',
        'default',
        {},
      );
      expect(getRootGroups(provider).map(group => group.state)).toEqual(['open', 'planned']);
      expect(provider.filterSummary).toBeUndefined();
    });
  });
});
