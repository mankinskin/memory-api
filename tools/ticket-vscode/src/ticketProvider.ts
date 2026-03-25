import * as vscode from 'vscode';
import { fetchAllTickets, type TicketSummary } from './api';

// Canonical ordering for ticket states in the tree.
const STATE_ORDER: ReadonlyArray<string> = [
  'open',
  'in-progress',
  'blocked',
  'review',
  'validating',
  'validated',
  'release-candidate',
  'released',
  'monitoring',
  'done',
  'cancelled',
];

const STATE_ICONS: Record<string, string> = {
  'open': 'circle-outline',
  'in-progress': 'loading~spin',
  'blocked': 'error',
  'review': 'eye',
  'validating': 'beaker',
  'validated': 'check',
  'release-candidate': 'rocket',
  'released': 'package',
  'monitoring': 'pulse',
  'done': 'pass-filled',
  'cancelled': 'circle-slash',
};

// ── Tree item types ──────────────────────────────────────────────────────────

/** Root-level item representing a state category, e.g. "open (3)". */
export class StateGroupItem extends vscode.TreeItem {
  readonly kind = 'stateGroup' as const;

  constructor(
    public readonly state: string,
    public readonly tickets: TicketSummary[],
  ) {
    super(
      `${state} (${tickets.length})`,
      vscode.TreeItemCollapsibleState.Collapsed,
    );
    this.contextValue = 'stateGroup';
    this.iconPath = new vscode.ThemeIcon(STATE_ICONS[state] ?? 'tag');
  }
}

/** Leaf item representing a single ticket. */
export class TicketItem extends vscode.TreeItem {
  readonly kind = 'ticket' as const;

  constructor(public readonly ticket: TicketSummary) {
    const label = ticket.title ?? `(${ticket.id.slice(0, 8)})`;
    super(label, vscode.TreeItemCollapsibleState.None);
    this.contextValue = 'ticket';
    this.id = `ticket:${ticket.id}`;
    this.description = ticket.id.slice(0, 8);
    this.tooltip = new vscode.MarkdownString(
      `**${label}**\n\nID: \`${ticket.id}\`\nState: ${ticket.state ?? '—'}\nType: ${ticket.type}`,
    );
    this.iconPath = new vscode.ThemeIcon('tag');
    this.command = {
      command: 'ticket-viewer.openTicket',
      title: 'Open Ticket',
      arguments: [this],
    };
  }
}

/** Informational placeholder (loading, error, empty). */
class InfoItem extends vscode.TreeItem {
  readonly kind = 'info' as const;

  constructor(label: string, icon: string, tooltip?: string) {
    super(label, vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon(icon);
    this.contextValue = 'info';
    if (tooltip) { this.tooltip = tooltip; }
  }
}

type TreeNode = StateGroupItem | TicketItem | InfoItem;

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

  private _baseUrl: string;
  private _workspace: string;
  private _autoRefreshSec: number;

  constructor(baseUrl: string, workspace: string, autoRefreshSec: number) {
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

  refresh(): void {
    void this.load();
  }

  /** Update connection settings and reload. */
  update(baseUrl: string, workspace: string, autoRefreshSec: number): void {
    this._baseUrl = baseUrl;
    this._workspace = workspace;
    this._autoRefreshSec = autoRefreshSec;
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
    return element;
  }

  getChildren(element?: TreeNode): TreeNode[] {
    if (element instanceof StateGroupItem) {
      return element.tickets.map(t => new TicketItem(t));
    }

    // Root level
    if (element !== undefined) { return []; }

    if (this.state === 'loading' && this.tickets.length === 0) {
      return [new InfoItem('Loading tickets…', 'loading~spin')];
    }

    if (this.state === 'error') {
      return [
        new InfoItem(
          'Server not reachable',
          'error',
          `Could not connect to ${this._baseUrl}\n\n${this.errorMessage}\n\nUse the ▶ button to start the server task.`,
        ),
      ];
    }

    if (this.tickets.length === 0) {
      return [new InfoItem('No tickets found', 'info')];
    }

    return this.buildStateGroups();
  }

  // ── Private ────────────────────────────────────────────────────────────────

  private buildStateGroups(): StateGroupItem[] {
    const grouped = new Map<string, TicketSummary[]>();

    for (const ticket of this.tickets) {
      const s = ticket.state ?? 'unknown';
      let bucket = grouped.get(s);
      if (!bucket) { bucket = []; grouped.set(s, bucket); }
      bucket.push(ticket);
    }

    const result: StateGroupItem[] = [];

    // Canonical order first.
    for (const s of STATE_ORDER) {
      const bucket = grouped.get(s);
      if (bucket && bucket.length > 0) {
        result.push(new StateGroupItem(s, bucket));
        grouped.delete(s);
      }
    }

    // Then any remaining unknown states alphabetically.
    for (const [s, bucket] of [...grouped.entries()].sort(([a], [b]) => a.localeCompare(b))) {
      if (bucket.length > 0) {
        result.push(new StateGroupItem(s, bucket));
      }
    }

    return result;
  }

  private async load(): Promise<void> {
    this.state = 'loading';
    this._onDidChangeTreeData.fire(undefined);

    try {
      this.tickets = await fetchAllTickets(this._baseUrl, this._workspace);
      this.state = 'idle';
      this.errorMessage = '';
    } catch (err) {
      this.errorMessage = err instanceof Error ? err.message : String(err);
      this.state = 'error';
      this.tickets = [];
    }

    this._onDidChangeTreeData.fire(undefined);
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
