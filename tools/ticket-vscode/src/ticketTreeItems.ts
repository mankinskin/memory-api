import * as path from 'node:path';
import * as vscode from 'vscode';

import type { TicketSummary } from './api';

/** Best-effort icon map for well-known states; unknown states get 'tag'. */
const STATE_ICONS: Record<string, string> = {
  'new': 'circle-outline',
  'ready': 'circle-large-outline',
  'in-implementation': 'tools',
  'in-review': 'eye',
  'done': 'pass-filled',
  'cancelled': 'circle-slash',
};

/** Root-level item representing a state category, e.g. "open (3)". */
export class StateGroupItem extends vscode.TreeItem {
  readonly kind = 'stateGroup' as const;

  constructor(
    public readonly state: string,
    public readonly totalCount: number,
    public readonly rootTickets: TicketSummary[],
  ) {
    super(
      `${state} (${totalCount})`,
      vscode.TreeItemCollapsibleState.Collapsed,
    );
    this.contextValue = 'stateGroup';
    this.iconPath = new vscode.ThemeIcon(STATE_ICONS[state] ?? 'tag');
  }
}

/** Always-visible root-level control for search/filter actions. */
export class FilterControlItem extends vscode.TreeItem {
  readonly kind = 'filterControl' as const;

  constructor(
    label: string,
    description: string,
    icon: string,
    commandId: string,
  ) {
    super(label, vscode.TreeItemCollapsibleState.None);
    this.description = description;
    this.contextValue = 'filterControl';
    this.iconPath = new vscode.ThemeIcon(icon);
    this.command = {
      command: commandId,
      title: label,
    };
  }
}

/** Item representing a single ticket. Collapsible when it has dependency children. */
export class TicketItem extends vscode.TreeItem {
  readonly kind = 'ticket' as const;

  constructor(
    public readonly ticket: TicketSummary,
    hasChildren: boolean = false,
    treePath?: string,
  ) {
    const label = ticket.title ?? `(${ticket.id.slice(0, 8)})`;
    super(
      label,
      // Always collapsible — even without dep children, folder contents are shown.
      vscode.TreeItemCollapsibleState.Collapsed,
    );
    this.contextValue = 'ticket';
    this.id = treePath ?? `ticket:${ticket.id}`;
    this.description = ticket.id.slice(0, 8);
    // Leave tooltip undefined so VS Code calls resolveTreeItem on hover,
    // which lazily fetches the description and sets the rich tooltip.
    this.iconPath = new vscode.ThemeIcon('tag');
    this.command = {
      command: 'ticket-viewer.openTicket',
      title: 'Open Ticket',
      arguments: [this],
    };
  }
}

/** Informational placeholder (loading, error, empty). */
export class InfoItem extends vscode.TreeItem {
  readonly kind = 'info' as const;

  constructor(label: string, icon: string, tooltip?: string) {
    super(label, vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon(icon);
    this.contextValue = 'info';
    if (tooltip) { this.tooltip = tooltip; }
  }
}

/** A file inside a ticket folder (leaf). */
export class TicketFileItem extends vscode.TreeItem {
  readonly kind = 'ticketFile' as const;

  constructor(public readonly filePath: string) {
    super(path.basename(filePath), vscode.TreeItemCollapsibleState.None);
    this.resourceUri = vscode.Uri.file(filePath);
    this.contextValue = 'ticketFile';
    this.command = {
      command: 'vscode.open',
      title: 'Open File',
      arguments: [this.resourceUri],
    };
  }
}

/** A subdirectory inside a ticket folder (expandable). */
export class TicketFolderItem extends vscode.TreeItem {
  readonly kind = 'ticketFolder' as const;

  constructor(public readonly folderPath: string) {
    super(path.basename(folderPath), vscode.TreeItemCollapsibleState.Collapsed);
    this.resourceUri = vscode.Uri.file(folderPath);
    this.contextValue = 'ticketFolder';
    this.iconPath = new vscode.ThemeIcon('folder');
  }
}

export type TreeNode =
  | FilterControlItem
  | StateGroupItem
  | TicketItem
  | TicketFileItem
  | TicketFolderItem
  | InfoItem;