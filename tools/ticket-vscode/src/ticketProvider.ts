import * as vscode from 'vscode';
import * as fs from 'node:fs';
import * as path from 'node:path';
import {
  fetchAllTickets,
  fetchEdges,
  fetchSchemas,
  fetchTicketDescription,
  type EdgeRecord,
  type TicketListFilters,
  type TicketSummary,
} from './api';
import {
  FilterControlItem,
  InfoItem,
  StateGroupItem,
  TicketFileItem,
  TicketFolderItem,
  TicketItem,
  type TreeNode,
} from './ticketTreeItems';

export { StateGroupItem, TicketItem } from './ticketTreeItems';

function normalizeFilterValue(value: string | undefined): string | undefined {
  const normalized = value?.trim();
  return normalized ? normalized : undefined;
}

// ── Provider ─────────────────────────────────────────────────────────────────

export class TicketTreeProvider
  implements vscode.TreeDataProvider<TreeNode>, vscode.Disposable
{
  private readonly _onDidChangeTreeData =
    new vscode.EventEmitter<TreeNode | undefined | null>();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  private tickets: TicketSummary[] = [];
  private state: 'idle' | 'loading' | 'error' = 'idle';
  private errorMessage = '';
  private refreshTimer: ReturnType<typeof setInterval> | undefined;
  private _descriptionCache = new Map<string, string | null>();
  /** Ordered state names from the schema endpoint; undefined until first fetch. */
  private _schemaStates: string[] | undefined;
  private _filters: TicketListFilters = {};

  /** Map from ticket ID to TicketSummary for quick lookup. */
  private _ticketMap = new Map<string, TicketSummary>();
  /** Map from ticket ID to the IDs of tickets it depends on (outgoing depends_on). */
  private _depsOf = new Map<string, string[]>();
  /** Set of ticket IDs that are the target of at least one depends_on edge. */
  private _hasParent = new Set<string>();
  /** Map from child ticket ID to the IDs of its parent tickets (reverse of _depsOf). */
  private _parentOf = new Map<string, string[]>();

  /** Absolute path to the .ticket/tickets/ directory on disk, or undefined if not found. */
  private _ticketsDir: string | undefined;

  private _baseUrl: string;
  private _workspace: string;
  private _autoRefreshSec: number;

  constructor(baseUrl: string, workspace: string, autoRefreshSec: number, ticketsDir?: string) {
    this._ticketsDir = ticketsDir;
    this._baseUrl = baseUrl;
    this._workspace = workspace;
    this._autoRefreshSec = autoRefreshSec;
    this.scheduleAutoRefresh();
    void this.load();
  }

  // ── Public API ─────────────────────────────────────────────────────────────

  /** Returns the current in-memory ticket list (used for status bar). */
  get allTickets(): ReadonlyArray<TicketSummary> {
    return this.tickets;
  }

  get filters(): Readonly<TicketListFilters> {
    return { ...this._filters };
  }

  get availableStates(): readonly string[] {
    if (this._schemaStates && this._schemaStates.length > 0) {
      return this._schemaStates;
    }
    return [...new Set(this.tickets
      .map(ticket => ticket.state)
      .filter((state): state is string => Boolean(state)))].sort((a, b) => a.localeCompare(b));
  }

  get filterSummary(): string | undefined {
    const parts: string[] = [];
    if (this._filters.query) {
      parts.push(`query="${this._filters.query}"`);
    }
    if (this._filters.state) {
      parts.push(`state=${this._filters.state}`);
    }
    return parts.length > 0 ? parts.join(', ') : undefined;
  }

  refresh(): void {
    this._descriptionCache.clear();
    void this.load();
  }

  setSearchQuery(query: string): void {
    this._setFilters({ ...this._filters, query });
  }

  setStateFilter(state: string | undefined): void {
    this._setFilters({ ...this._filters, state });
  }

  clearFilters(): void {
    this._setFilters({});
  }

  /** Update connection settings and reload. */
  update(baseUrl: string, workspace: string, autoRefreshSec: number, ticketsDir?: string): void {
    this._baseUrl = baseUrl;
    this._workspace = workspace;
    this._autoRefreshSec = autoRefreshSec;
    this._ticketsDir = ticketsDir;
    this._descriptionCache.clear();
    this.scheduleAutoRefresh();
    void this.load();
  }

  dispose(): void {
    if (this.refreshTimer !== undefined) {
      clearInterval(this.refreshTimer);
    }
    this._onDidChangeTreeData.dispose();
  }

  // ── vscode.TreeDataProvider ────────────────────────────────────────────────

  getTreeItem(element: TreeNode): vscode.TreeItem {
    // Clear any previously-set tooltip so VS Code calls resolveTreeItem again
    // on the next hover instead of using the cached rich tooltip instantly.
    if (this._lastTooltipItem) {
      this._lastTooltipItem.tooltip = undefined;
      this._lastTooltipItem = undefined;
    }
    return element;
  }

  getChildren(element?: TreeNode): TreeNode[] {
    const filterControls = this._buildFilterControls();

    if (element instanceof StateGroupItem) {
      return element.rootTickets.map(t => this._makeTicketItem(t, element.state));
    }

    if (element instanceof TicketItem) {
      const depChildren = this._getDependencyChildren(element);
      const folderChildren = this._getTicketFolderChildren(element.ticket.id);
      return [...depChildren, ...folderChildren];
    }

    if (element instanceof TicketFolderItem) {
      return this._readDirEntries(element.folderPath);
    }

    // TicketFileItem and InfoItem are leaves.
    if (element !== undefined) { return []; }

    if (this.state === 'loading' && this.tickets.length === 0) {
      return [...filterControls, new InfoItem('Loading tickets…', 'loading~spin')];
    }

    if (this.state === 'error') {
      return [
        ...filterControls,
        new InfoItem(
          'Server not reachable',
          'error',
          `Could not connect to ${this._baseUrl}\n\n${this.errorMessage}\n\nUse the ▶ button to start the server task.`,
        ),
      ];
    }

    if (this.tickets.length === 0) {
      const filterSummary = this.filterSummary;
      return [...filterControls, new InfoItem(
        filterSummary ? 'No tickets match current filters' : 'No tickets found',
        'info',
        filterSummary ? `Active filters: ${filterSummary}` : undefined,
      )];
    }

    return [...filterControls, ...this.buildStateGroups()];
  }

  // ── Lazy tooltip resolution ────────────────────────────────────────────────
  //
  // resolveTreeItem is called by VS Code on hover (when tooltip is undefined).
  // We resolve immediately — VS Code's own hover delay + CancellationToken
  // provide sufficient debouncing against cursor fly-bys.
  //
  // Important: we clear item.tooltip in getTreeItem so VS Code calls
  // resolveTreeItem again on every hover rather than caching a stale tooltip.

  /** Track the last item whose tooltip was set so we can clear it. */
  private _lastTooltipItem: TicketItem | undefined;

  async resolveTreeItem(
    item: TreeNode,
    _element: TreeNode,
    token: vscode.CancellationToken,
  ): Promise<TreeNode> {
    if (!(item instanceof TicketItem)) { return item; }

    // Clear tooltip from any previous hover so it doesn't stick.
    if (this._lastTooltipItem && this._lastTooltipItem !== item) {
      this._lastTooltipItem.tooltip = undefined;
    }

    const id = item.ticket.id;
    let desc = this._descriptionCache.get(id);
    if (desc === undefined) {
      try {
        desc = await fetchTicketDescription(this._baseUrl, this._workspace, id);
        if (token.isCancellationRequested) { return item; }
        this._descriptionCache.set(id, desc);
      } catch {
        desc = null;
        this._descriptionCache.set(id, null);
      }
    }

    if (token.isCancellationRequested) { return item; }

    this._setDescriptionTooltip(item, desc ?? null);
    this._lastTooltipItem = item;
    return item;
  }

  private _setDescriptionTooltip(item: TicketItem, description: string | null): void {
    const label = item.ticket.title ?? `(${item.ticket.id.slice(0, 8)})`;
    const meta = `**${label}**\n\nID: \`${item.ticket.id}\`\nState: ${item.ticket.state ?? '\u2014'}\nType: ${item.ticket.type}`;
    const body = description ? `\n\n---\n\n${description}` : '';
    const md = new vscode.MarkdownString(`${meta}${body}`, true);
    md.isTrusted = false;
    item.tooltip = md;
  }

  // ── Private ────────────────────────────────────────────────────────────────

  /** Create a TicketItem with correct collapsibility and a unique tree path. */
  private _makeTicketItem(ticket: TicketSummary, parentPath: string): TicketItem {
    const hasChildren = (this._depsOf.get(ticket.id)?.length ?? 0) > 0;
    const treePath = `${parentPath}|${ticket.id}`;
    return new TicketItem(ticket, hasChildren, treePath);
  }

  /** Return TicketItems for the dependencies of the given parent ticket, filtered to same state. */
  private _getDependencyChildren(parent: TicketItem): TicketItem[] {
    const depIds = this._depsOf.get(parent.ticket.id) ?? [];
    const parentState = parent.ticket.state;
    const children: TicketItem[] = [];
    for (const depId of depIds) {
      const ticket = this._ticketMap.get(depId);
      if (!ticket) { continue; }
      // Only show children that share the parent's state
      if (ticket.state === parentState) {
        children.push(this._makeTicketItem(ticket, parent.id ?? parent.ticket.id));
      }
    }
    return children;
  }

  /** Return file/folder entries for the ticket's on-disk folder. */
  private _getTicketFolderChildren(ticketId: string): (TicketFileItem | TicketFolderItem)[] {
    if (!this._ticketsDir) { return []; }
    const ticketDir = path.join(this._ticketsDir, ticketId);
    return this._readDirEntries(ticketDir);
  }

  /** Read a directory and return sorted TicketFolderItem / TicketFileItem nodes. */
  private _readDirEntries(dirPath: string): (TicketFileItem | TicketFolderItem)[] {
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dirPath, { withFileTypes: true });
    } catch {
      return [];
    }
    const folders: TicketFolderItem[] = [];
    const files: TicketFileItem[] = [];
    for (const entry of entries) {
      const full = path.join(dirPath, entry.name);
      if (entry.isDirectory()) {
        folders.push(new TicketFolderItem(full));
      } else if (entry.isFile()) {
        files.push(new TicketFileItem(full));
      }
    }
    folders.sort((a, b) => a.folderPath.localeCompare(b.folderPath));
    files.sort((a, b) => a.filePath.localeCompare(b.filePath));
    return [...folders, ...files];
  }

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

  private _buildFilterControls(): FilterControlItem[] {
    return [
      new FilterControlItem(
        'Search Tickets',
        this._filters.query ?? 'None',
        'search',
        'ticket-viewer.setSearchQuery',
      ),
      new FilterControlItem(
        'Filter By State',
        this._filters.state ?? 'All states',
        'filter',
        'ticket-viewer.setStateFilter',
      ),
    ];
  }

  private _setFilters(filters: TicketListFilters): void {
    const nextFilters: TicketListFilters = {
      query: normalizeFilterValue(filters.query),
      state: normalizeFilterValue(filters.state),
    };

    if (
      this._filters.query === nextFilters.query
      && this._filters.state === nextFilters.state
    ) {
      return;
    }

    this._filters = nextFilters;
    this._descriptionCache.clear();
    void this.load();
  }

  private async load(): Promise<void> {
    this.state = 'loading';
    this._onDidChangeTreeData.fire(undefined);

    try {
      const [tickets, edges, schemas] = await Promise.all([
        fetchAllTickets(this._baseUrl, this._workspace, this._filters),
        fetchEdges(this._baseUrl, this._workspace, 'depends_on').catch(() => [] as EdgeRecord[]),
        fetchSchemas(this._baseUrl, this._workspace).catch(() => []),
      ]);
      this._schemaStates = schemas.flatMap(s => s.states);
      this.tickets = tickets;
      this._buildDependencyMaps(edges);
      this.state = 'idle';
      this.errorMessage = '';
    } catch (err) {
      this.errorMessage = err instanceof Error ? err.message : String(err);
      this.state = 'error';
      this.tickets = [];
      this._ticketMap.clear();
      this._depsOf.clear();
      this._hasParent.clear();
    }

    this._onDidChangeTreeData.fire(undefined);
  }

  /** Build lookup maps from the fetched edges. */
  private _buildDependencyMaps(edges: EdgeRecord[]): void {
    this._ticketMap.clear();
    this._depsOf.clear();
    this._hasParent.clear();
    this._parentOf.clear();

    for (const t of this.tickets) {
      this._ticketMap.set(t.id, t);
    }

    for (const edge of edges) {
      // edge.from depends_on edge.to → from is parent, to is child in the tree
      if (!this._ticketMap.has(edge.from) || !this._ticketMap.has(edge.to)) {
        continue; // skip edges referencing unknown tickets
      }
      let deps = this._depsOf.get(edge.from);
      if (!deps) { deps = []; this._depsOf.set(edge.from, deps); }
      deps.push(edge.to);
      this._hasParent.add(edge.to);

      let parents = this._parentOf.get(edge.to);
      if (!parents) { parents = []; this._parentOf.set(edge.to, parents); }
      parents.push(edge.from);
    }
  }

  private scheduleAutoRefresh(): void {
    if (this.refreshTimer !== undefined) {
      clearInterval(this.refreshTimer);
      this.refreshTimer = undefined;
    }
    if (this._autoRefreshSec > 0) {
      this.refreshTimer = setInterval(
        () => void this.load(),
        this._autoRefreshSec * 1000,
      );
    }
  }
}
