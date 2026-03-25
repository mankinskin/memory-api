"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.TicketTreeProvider = exports.TicketItem = exports.StateGroupItem = void 0;
const vscode = __importStar(require("vscode"));
const api_1 = require("./api");
// Canonical ordering for ticket states in the tree.
const STATE_ORDER = [
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
const STATE_ICONS = {
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
class StateGroupItem extends vscode.TreeItem {
    constructor(state, tickets) {
        super(`${state} (${tickets.length})`, vscode.TreeItemCollapsibleState.Collapsed);
        this.state = state;
        this.tickets = tickets;
        this.kind = 'stateGroup';
        this.contextValue = 'stateGroup';
        this.iconPath = new vscode.ThemeIcon(STATE_ICONS[state] ?? 'tag');
    }
}
exports.StateGroupItem = StateGroupItem;
/** Leaf item representing a single ticket. */
class TicketItem extends vscode.TreeItem {
    constructor(ticket) {
        const label = ticket.title ?? `(${ticket.id.slice(0, 8)})`;
        super(label, vscode.TreeItemCollapsibleState.None);
        this.ticket = ticket;
        this.kind = 'ticket';
        this.contextValue = 'ticket';
        this.id = `ticket:${ticket.id}`;
        this.description = ticket.id.slice(0, 8);
        this.tooltip = new vscode.MarkdownString(`**${label}**\n\nID: \`${ticket.id}\`\nState: ${ticket.state ?? '—'}\nType: ${ticket.type}`);
        this.iconPath = new vscode.ThemeIcon('tag');
        this.command = {
            command: 'ticket-viewer.openTicket',
            title: 'Open Ticket',
            arguments: [this],
        };
    }
}
exports.TicketItem = TicketItem;
/** Informational placeholder (loading, error, empty). */
class InfoItem extends vscode.TreeItem {
    constructor(label, icon, tooltip) {
        super(label, vscode.TreeItemCollapsibleState.None);
        this.kind = 'info';
        this.iconPath = new vscode.ThemeIcon(icon);
        this.contextValue = 'info';
        if (tooltip) {
            this.tooltip = tooltip;
        }
    }
}
// ── Provider ─────────────────────────────────────────────────────────────────
class TicketTreeProvider {
    constructor(baseUrl, workspace, autoRefreshSec) {
        this._onDidChangeTreeData = new vscode.EventEmitter();
        this.onDidChangeTreeData = this._onDidChangeTreeData.event;
        this.tickets = [];
        this.state = 'idle';
        this.errorMessage = '';
        this._baseUrl = baseUrl;
        this._workspace = workspace;
        this._autoRefreshSec = autoRefreshSec;
        this.scheduleAutoRefresh();
        void this.load();
    }
    // ── Public API ─────────────────────────────────────────────────────────────
    /** Returns the current in-memory ticket list (used for status bar). */
    get allTickets() {
        return this.tickets;
    }
    refresh() {
        void this.load();
    }
    /** Update connection settings and reload. */
    update(baseUrl, workspace, autoRefreshSec) {
        this._baseUrl = baseUrl;
        this._workspace = workspace;
        this._autoRefreshSec = autoRefreshSec;
        this.scheduleAutoRefresh();
        void this.load();
    }
    dispose() {
        if (this.refreshTimer !== undefined) {
            clearInterval(this.refreshTimer);
        }
        this._onDidChangeTreeData.dispose();
    }
    // ── vscode.TreeDataProvider ────────────────────────────────────────────────
    getTreeItem(element) {
        return element;
    }
    getChildren(element) {
        if (element instanceof StateGroupItem) {
            return element.tickets.map(t => new TicketItem(t));
        }
        // Root level
        if (element !== undefined) {
            return [];
        }
        if (this.state === 'loading' && this.tickets.length === 0) {
            return [new InfoItem('Loading tickets…', 'loading~spin')];
        }
        if (this.state === 'error') {
            return [
                new InfoItem('Server not reachable', 'error', `Could not connect to ${this._baseUrl}\n\n${this.errorMessage}\n\nUse the ▶ button to start the server task.`),
            ];
        }
        if (this.tickets.length === 0) {
            return [new InfoItem('No tickets found', 'info')];
        }
        return this.buildStateGroups();
    }
    // ── Private ────────────────────────────────────────────────────────────────
    buildStateGroups() {
        const grouped = new Map();
        for (const ticket of this.tickets) {
            const s = ticket.state ?? 'unknown';
            let bucket = grouped.get(s);
            if (!bucket) {
                bucket = [];
                grouped.set(s, bucket);
            }
            bucket.push(ticket);
        }
        const result = [];
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
    async load() {
        this.state = 'loading';
        this._onDidChangeTreeData.fire(undefined);
        try {
            this.tickets = await (0, api_1.fetchAllTickets)(this._baseUrl, this._workspace);
            this.state = 'idle';
            this.errorMessage = '';
        }
        catch (err) {
            this.errorMessage = err instanceof Error ? err.message : String(err);
            this.state = 'error';
            this.tickets = [];
        }
        this._onDidChangeTreeData.fire(undefined);
    }
    scheduleAutoRefresh() {
        if (this.refreshTimer !== undefined) {
            clearInterval(this.refreshTimer);
            this.refreshTimer = undefined;
        }
        if (this._autoRefreshSec > 0) {
            this.refreshTimer = setInterval(() => void this.load(), this._autoRefreshSec * 1000);
        }
    }
}
exports.TicketTreeProvider = TicketTreeProvider;
//# sourceMappingURL=ticketProvider.js.map