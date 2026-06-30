import * as vscode from 'vscode';
import type { WorkspaceFsCapability } from './hostCapabilities';
import type { CoreApi } from './coreLoader';
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

function isApiRequestError(error: unknown): error is {
  operation: string;
  url: string;
  method: string;
  status?: number;
  responseBody?: string;
  message: string;
} {
  if (!error || typeof error !== 'object') {
    return false;
  }

  const candidate = error as Record<string, unknown>;
  return typeof candidate.operation === 'string'
    && typeof candidate.url === 'string'
    && typeof candidate.method === 'string'
    && typeof candidate.message === 'string';
}

function formatErrorStateMessage(
  baseUrl: string,
  workspace: string,
  filters: TicketListFilters,
  error: unknown,
): { label: string; tooltip: string } {
  const lines = [
    `Server URL: ${baseUrl}`,
    `Workspace: ${workspace}`,
  ];
  const filterSummary = [
    filters.query ? `query=${filters.query}` : undefined,
    filters.state ? `state=${filters.state}` : undefined,
  ].filter((value): value is string => Boolean(value));
  lines.push(`Filters: ${filterSummary.length > 0 ? filterSummary.join(', ') : 'none'}`);

  if (isApiRequestError(error)) {
    lines.push(`Operation: ${error.operation}`);
    lines.push(`Request: ${error.method} ${error.url}`);
    if (typeof error.status === 'number') {
      lines.push(`HTTP status: ${error.status}`);
    }
    if (error.responseBody && error.responseBody !== '') {
      lines.push(`Response: ${error.responseBody}`);
    }
    const statusSuffix = typeof error.status === 'number' ? ` (HTTP ${error.status})` : '';
    return {
      label: `Request failed${statusSuffix}: ${error.operation}`,
      tooltip: `${lines.join('\n')}\n\n${error.message}\n\nUse the ▶ button to start or reconnect the server task.`,
    };
  }

  const message = error instanceof Error ? error.message : String(error);
  return {
    label: 'Server not reachable',
    tooltip: `${lines.join('\n')}\n\n${message}\n\nUse the ▶ button to start or reconnect the server task.`,
  };
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
  private errorLabel = 'Server not reachable';
  private refreshTimer: ReturnType<typeof setInterval> | undefined;
  private _descriptionCache = new Map<string, string | null>();
  /** Ordered state names from the schema endpoint; undefined until first fetch. */
  private _schemaStates: string[] | undefined;
  private _filters: TicketListFilters = {};
  /** Client-side substring filter applied live while the search InputBox is open. No server call. */
  private _localSearch: string = '';

  /** Map from ticket ID to TicketSummary for quick lookup. */
  private _ticketMap = new Map<string, TicketSummary>();
  /** Map from ticket ID to the IDs of tickets it depends on (outgoing depends_on). */
  private _depsOf = new Map<string, string[]>();
  /** Map from child ticket ID to the IDs of its parent tickets (reverse of _depsOf). */
  private _parentOf = new Map<string, string[]>();

  /** Rust/WASM core — undefined only when WASM failed to load. */
  private _core: CoreApi | undefined;

  /**
   * URI of the .ticket/tickets/ directory, or undefined if not found.
   * Replaces the old string path so virtual/web workspace URIs resolve too.
   */
  private _ticketsDirUri: vscode.Uri | undefined;
  /** Capability adapter for file browsing — undefined in browser/virtual hosts. */
  private _workspaceFs: WorkspaceFsCapability | undefined;

  private _baseUrl: string;
  private _workspace: string;
  private _autoRefreshSec: number;
  private readonly _recoverConnection?: (
    error: unknown,
  ) => Promise<{ baseUrl: string; workspace: string; ticketsDirUri?: vscode.Uri } | undefined>;

  constructor(
    baseUrl: string,
    workspace: string,
    autoRefreshSec: number,
    ticketsDirUri?: vscode.Uri,
    recoverConnection?: (
      error: unknown,
    ) => Promise<{ baseUrl: string; workspace: string; ticketsDirUri?: vscode.Uri } | undefined>,
    workspaceFs?: WorkspaceFsCapability,
    core?: CoreApi,
  ) {
    this._ticketsDirUri = ticketsDirUri;
    this._workspaceFs = workspaceFs;
    this._baseUrl = baseUrl;
    this._workspace = workspace;
    this._autoRefreshSec = autoRefreshSec;
    this._recoverConnection = recoverConnection;
    this._core = core;
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
    if (this._localSearch) {
      parts.push(`search="${this._localSearch}"`);
    } else if (this._filters.query) {
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

  /** Apply a client-side substring filter immediately. Fires a tree refresh without a server call. */
  setLocalSearch(query: string): void {
    this._localSearch = query;
    this._onDidChangeTreeData.fire(undefined);
  }

  /** Update connection settings and reload. */
  update(baseUrl: string, workspace: string, autoRefreshSec: number, ticketsDirUri?: vscode.Uri, workspaceFs?: WorkspaceFsCapability, core?: CoreApi): void {
    this._baseUrl = baseUrl;
    this._workspace = workspace;
    this._autoRefreshSec = autoRefreshSec;
    this._ticketsDirUri = ticketsDirUri;
    this._workspaceFs = workspaceFs;
    if (core !== undefined) { this._core = core; }
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
      // Async via WorkspaceFsCapability; returns [] synchronously and fires a
      // tree refresh once the promise resolves (browser/virtual = always []).
      void this._readDirEntriesAsync(vscode.Uri.file(element.folderPath));
      return [];
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
          this.errorLabel,
          'error',
          this.errorMessage,
          // Pass copyText so the item gets contextValue 'error-info' and the
          // copy command has something to put on the clipboard.
          this.errorMessage,
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

    const groups = this.buildStateGroups();
    if (groups.length === 0) {
      return [...filterControls, new InfoItem(`No tickets match "${this._localSearch}"`, 'info')];
    }
    return [...filterControls, ...groups];
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
    const label = this._core
      ? this._core.ticket_display_label(item.ticket.id, item.ticket.title)
      : (item.ticket.title ?? `(${item.ticket.id.slice(0, 8)})`);
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

  /** Return file/folder entries for the ticket's directory. */
  private _getTicketFolderChildren(ticketId: string): (TicketFileItem | TicketFolderItem)[] {
    if (!this._ticketsDirUri) { return []; }
    const ticketDirUri = vscode.Uri.joinPath(this._ticketsDirUri, ticketId);
    // Kick off async read; tree will refresh when the promise resolves.
    void this._readDirEntriesAsync(ticketDirUri);
    return [];
  }

  /**
   * Async directory read via WorkspaceFsCapability.
   *
   * Rule 3 (frozen): no node:fs or node:path. WorkspaceFsCapability is absent
   * in browser/virtual hosts — those hosts return an empty list silently.
   */
  private async _readDirEntriesAsync(
    dirUri: vscode.Uri,
  ): Promise<(TicketFileItem | TicketFolderItem)[]> {
    if (!this._workspaceFs) { return []; }
    let entries: [string, vscode.FileType][];
    try {
      entries = await this._workspaceFs.readDirectory(dirUri);
    } catch {
      return [];
    }
    const folders: TicketFolderItem[] = [];
    const files: TicketFileItem[] = [];
    for (const [name, fileType] of entries) {
      const childUri = vscode.Uri.joinPath(dirUri, name);
      if (fileType === vscode.FileType.Directory) {
        folders.push(new TicketFolderItem(childUri.fsPath));
      } else if (fileType === vscode.FileType.File) {
        files.push(new TicketFileItem(childUri.fsPath));
      }
    }
    folders.sort((a, b) => a.folderPath.localeCompare(b.folderPath));
    files.sort((a, b) => a.filePath.localeCompare(b.filePath));
    return [...folders, ...files];
  }

  private buildStateGroups(): StateGroupItem[] {
    // Determine the active edge set and state order for grouping.
    const edges: EdgeRecord[] = [];
    for (const [from, depIds] of this._depsOf) {
      for (const to of depIds) {
        edges.push({ from, to, kind: 'depends_on' });
      }
    }
    const stateOrder = this._schemaStates ?? [];
    const query = this._localSearch.trim();
    const groups = this._requireCore().build_state_groups(
      this.tickets, edges, stateOrder, query,
    );
    return groups.map(g => {
      const rootTickets = g.rootIds
        .map(id => this._ticketMap.get(id))
        .filter((t): t is TicketSummary => t !== undefined);
      return new StateGroupItem(g.state, g.total, rootTickets);
    });
  }

  private _buildFilterControls(): FilterControlItem[] {
    return [
      new FilterControlItem(
        'Search Tickets',
        this._localSearch || this._filters.query || 'None',
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

  private async load(allowRecovery = true): Promise<void> {
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
      this.errorLabel = 'Server not reachable';
    } catch (err) {
      if (allowRecovery && this._recoverConnection) {
        const recovered = await this._recoverConnection(err);
        if (recovered) {
          this._baseUrl = recovered.baseUrl;
          this._workspace = recovered.workspace;
          this._ticketsDirUri = recovered.ticketsDirUri;
          return this.load(false);
        }
      }

      const formattedError = formatErrorStateMessage(
        this._baseUrl,
        this._workspace,
        this._filters,
        err,
      );
      this.errorLabel = formattedError.label;
      this.errorMessage = formattedError.tooltip;
      this.state = 'error';
      this.tickets = [];
      this._ticketMap.clear();
      this._depsOf.clear();
      this._parentOf.clear();
    }

    this._onDidChangeTreeData.fire(undefined);
  }

  /** Build lookup maps from the fetched edges using the WASM core. */
  private _buildDependencyMaps(edges: EdgeRecord[]): void {
    this._ticketMap.clear();
    this._depsOf.clear();
    this._parentOf.clear();

    for (const t of this.tickets) {
      this._ticketMap.set(t.id, t);
    }
    const result = this._requireCore().build_dependency_maps(this.tickets, edges);
    for (const [id, deps] of result.depsOf) { this._depsOf.set(id, deps); }
    for (const [id, parents] of result.parentOf) { this._parentOf.set(id, parents); }
  }

  private _requireCore(): CoreApi {
    if (!this._core) {
      throw new Error('TicketTreeProvider requires the WASM core');
    }
    return this._core;
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
