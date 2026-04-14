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
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const fs = __importStar(require("node:fs"));
const path = __importStar(require("node:path"));
const ticketProvider_1 = require("./ticketProvider");
const api_1 = require("./api");
const browserBridge_1 = require("./browserBridge");
function readConfig() {
    const cfg = vscode.workspace.getConfiguration('ticketViewer');
    return {
        serverUrl: cfg.get('serverUrl', 'http://localhost:3002'),
        workspace: cfg.get('workspace', ''),
        autoRefreshSeconds: cfg.get('autoRefreshSeconds', 30),
        autoStartServer: cfg.get('autoStartServer', true),
        bridgePort: cfg.get('bridgePort', 0),
        cdpPort: cfg.get('cdpPort', 0),
        autoConnectCdp: cfg.get('autoConnectCdp', true),
    };
}
function detectTicketWorkspaces() {
    const folders = vscode.workspace.workspaceFolders ?? [];
    return folders.flatMap(folder => {
        const ticketDir = path.join(folder.uri.fsPath, '.ticket');
        try {
            if (fs.statSync(ticketDir).isDirectory()) {
                return [{ folderName: folder.name, ticketPath: ticketDir, folder }];
            }
        }
        catch { /* directory not found */ }
        return [];
    });
}
/** Resolve the active workspace name and a display label for the status bar. */
async function resolveActiveWorkspace(serverUrl, configured, context) {
    // Explicit user config always wins.
    if (configured.trim() !== '') {
        return { workspace: configured.trim(), displayName: configured.trim() };
    }
    // Scan open VS Code folders for .ticket/ directories.
    const detected = detectTicketWorkspaces();
    // Fetch workspace names the server currently knows.
    let serverWorkspaces = [];
    try {
        const list = await (0, api_1.fetchWorkspaces)(serverUrl);
        serverWorkspaces = list.map(w => w.name);
    }
    catch { /* server may not be running yet */ }
    if (detected.length === 1) {
        const { folderName } = detected[0];
        const wsName = serverWorkspaces.includes(folderName)
            ? folderName
            : (serverWorkspaces[0] ?? 'default');
        return { workspace: wsName, displayName: folderName };
    }
    if (detected.length > 1) {
        // Restore a previously chosen folder for this window.
        const stored = context.workspaceState.get('activeTicketFolder');
        if (stored) {
            const match = detected.find(d => d.folderName === stored);
            if (match) {
                const wsName = serverWorkspaces.includes(match.folderName)
                    ? match.folderName
                    : (serverWorkspaces[0] ?? 'default');
                return { workspace: wsName, displayName: match.folderName };
            }
        }
        // Multiple candidates — ask the user.
        const items = detected.map(d => ({
            label: d.folderName,
            description: d.ticketPath,
            folderName: d.folderName,
        }));
        const pick = await vscode.window.showQuickPick(items, {
            placeHolder: 'Multiple .ticket workspaces found — select one',
            title: 'Active Ticket Workspace',
        });
        if (pick) {
            await context.workspaceState.update('activeTicketFolder', pick.folderName);
            const wsName = serverWorkspaces.includes(pick.folderName)
                ? pick.folderName
                : (serverWorkspaces[0] ?? 'default');
            return { workspace: wsName, displayName: pick.folderName };
        }
    }
    // Fallback: first workspace from server.
    const ws = serverWorkspaces[0] ?? 'default';
    return { workspace: ws, displayName: ws };
}
function openTicketViewer(url) {
    void vscode.commands.executeCommand('simpleBrowser.show', url);
}
async function startServerTask() {
    // Invoke the ticket-viewer: start task defined in .vscode/tasks.json.
    try {
        await vscode.commands.executeCommand('workbench.action.tasks.runTask', 'ticket-viewer: start');
    }
    catch {
        void vscode.window.showErrorMessage('Could not start "ticket-viewer: start" task. Make sure .vscode/tasks.json is configured.');
    }
}
async function activate(context) {
    let config = readConfig();
    let { workspace, displayName } = await resolveActiveWorkspace(config.serverUrl, config.workspace, context);
    // ── Auto-start server ────────────────────────────────────────────
    if (config.autoStartServer) {
        void startServerTask();
    }
    // ── Tree data provider ──────────────────────────────────────────
    const provider = new ticketProvider_1.TicketTreeProvider(config.serverUrl, workspace, config.autoRefreshSeconds);
    context.subscriptions.push(provider);
    // If we auto-started the server give it a moment then retry.
    if (config.autoStartServer) {
        setTimeout(() => provider.refresh(), 3000);
    }
    const treeView = vscode.window.createTreeView('ticket-viewer.tickets', {
        treeDataProvider: provider,
        showCollapseAll: true,
    });
    context.subscriptions.push(treeView);
    // ── Status bar item ───────────────────────────────────────────────────────
    const statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    statusBarItem.command = 'ticket-viewer.openBrowser';
    statusBarItem.tooltip = `Open Ticket Viewer (${config.serverUrl})`;
    statusBarItem.show();
    context.subscriptions.push(statusBarItem);
    function updateStatusBar() {
        const tickets = provider.allTickets;
        const openCount = tickets.filter(t => t.state === 'open').length;
        const inProgressCount = tickets.filter(t => t.state === 'in-progress').length;
        const prefix = `$(issues) ${displayName}`;
        if (tickets.length === 0) {
            statusBarItem.text = prefix;
        }
        else {
            const parts = [];
            if (openCount > 0) {
                parts.push(`${openCount} open`);
            }
            if (inProgressCount > 0) {
                parts.push(`${inProgressCount} in-progress`);
            }
            statusBarItem.text = parts.length > 0
                ? `${prefix}: ${parts.join(', ')}`
                : `${prefix} (${tickets.length})`;
        }
    }
    // Update status bar whenever the tree data changes.
    context.subscriptions.push(provider.onDidChangeTreeData(() => updateStatusBar()));
    // ── Commands ──────────────────────────────────────────────────────────────
    context.subscriptions.push(vscode.commands.registerCommand('ticket-viewer.openBrowser', () => {
        openTicketViewer(config.serverUrl);
    }));
    context.subscriptions.push(vscode.commands.registerCommand('ticket-viewer.refresh', () => {
        provider.refresh();
    }));
    context.subscriptions.push(vscode.commands.registerCommand('ticket-viewer.startServer', async () => {
        await startServerTask();
        // Give the server a moment to start, then refresh.
        setTimeout(() => provider.refresh(), 3000);
    }));
    context.subscriptions.push(vscode.commands.registerCommand('ticket-viewer.openTicket', (item) => {
        const ticketUrl = `${config.serverUrl}/#/ws/${encodeURIComponent(workspace)}/ticket/${encodeURIComponent(item.ticket.id)}`;
        openTicketViewer(ticketUrl);
    }));
    context.subscriptions.push(vscode.commands.registerCommand('ticket-viewer.copyId', (item) => {
        void vscode.env.clipboard.writeText(item.ticket.id).then(() => {
            void vscode.window.showInformationMessage(`Copied: ${item.ticket.id}`);
        });
    }));
    context.subscriptions.push(vscode.commands.registerCommand('ticket-viewer.selectWorkspace', async () => {
        const detected = detectTicketWorkspaces();
        if (detected.length === 0) {
            void vscode.window.showInformationMessage('No .ticket workspaces found in the currently open folders.');
            return;
        }
        const items = detected.map(d => ({
            label: d.folderName,
            description: d.ticketPath,
            folderName: d.folderName,
        }));
        const pick = await vscode.window.showQuickPick(items, {
            placeHolder: 'Select active ticket workspace',
            title: 'Active Ticket Workspace',
        });
        if (!pick) {
            return;
        }
        await context.workspaceState.update('activeTicketFolder', pick.folderName);
        const resolved = await resolveActiveWorkspace(config.serverUrl, config.workspace, context);
        workspace = resolved.workspace;
        displayName = resolved.displayName;
        provider.update(config.serverUrl, workspace, config.autoRefreshSeconds);
        updateStatusBar();
    }));
    // ── Browser Bridge ──────────────────────────────────────────────────────────
    const bridge = new browserBridge_1.BrowserBridge({
        controlPort: config.bridgePort,
        cdpPort: config.cdpPort,
        autoConnectCdp: config.autoConnectCdp,
    });
    context.subscriptions.push(bridge);
    // Start the control server immediately.
    bridge.start().catch(err => {
        const msg = err instanceof Error ? err.message : String(err);
        void vscode.window.showWarningMessage(`Browser Bridge failed to start: ${msg}`);
    });
    context.subscriptions.push(vscode.commands.registerCommand('ticket-viewer.bridgeNavigate', async () => {
        const url = await vscode.window.showInputBox({
            prompt: 'URL to open in Simple Browser',
            value: config.serverUrl,
        });
        if (url) {
            await bridge.navigate(url);
        }
    }));
    context.subscriptions.push(vscode.commands.registerCommand('ticket-viewer.bridgeConnectCdp', async () => {
        const ok = await bridge.connectCdp();
        if (ok) {
            void vscode.window.showInformationMessage('Browser Bridge: CDP connected.');
        }
    }));
    context.subscriptions.push(vscode.commands.registerCommand('ticket-viewer.bridgeStatus', () => {
        const s = bridge.state;
        void vscode.window.showInformationMessage(`Bridge port: ${s.controlPort} | CDP: ${s.cdpConnected ? 'connected' : 'disconnected'} | URL: ${s.currentUrl ?? 'none'}`);
    }));
    // ── React to VS Code folder changes ───────────────────────────────
    context.subscriptions.push(vscode.workspace.onDidChangeWorkspaceFolders(async () => {
        const resolved = await resolveActiveWorkspace(config.serverUrl, config.workspace, context);
        workspace = resolved.workspace;
        displayName = resolved.displayName;
        provider.update(config.serverUrl, workspace, config.autoRefreshSeconds);
        updateStatusBar();
    }));
    // ── React to config changes ───────────────────────────────────────────
    context.subscriptions.push(vscode.workspace.onDidChangeConfiguration(async (e) => {
        if (!e.affectsConfiguration('ticketViewer')) {
            return;
        }
        config = readConfig();
        const resolved = await resolveActiveWorkspace(config.serverUrl, config.workspace, context);
        workspace = resolved.workspace;
        displayName = resolved.displayName;
        provider.update(config.serverUrl, workspace, config.autoRefreshSeconds);
        statusBarItem.tooltip = `Open Ticket Viewer (${config.serverUrl})`;
        updateStatusBar();
    }));
}
function deactivate() {
    // Nothing to clean up beyond subscriptions (handled by context.subscriptions).
}
//# sourceMappingURL=extension.js.map