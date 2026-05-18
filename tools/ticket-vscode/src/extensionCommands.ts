import * as vscode from 'vscode';
import * as fs from 'node:fs';
import * as path from 'node:path';
import type { ChildProcess } from 'node:child_process';

import { BrowserBridge } from './browserBridge';
import { addEdge, cancelTicket, closeTicket, createTicket, deleteTicket, undoTicket, updateTicket } from './api';
import { TicketItem, TicketTreeProvider } from './ticketProvider';
import { TICKET_STATES, TICKET_TYPES, detectTicketWorkspaces, openTicketViewer, pingServer, pollUntilReachable, readConfig, resolveActiveWorkspace, resolveTicketsDir, startServerTask, type TicketViewerConfig } from './extensionSupport';

export interface ActivationState {
  config: TicketViewerConfig;
  serverUrl: string;
  workspace: string;
  displayName: string;
}

interface RegisterExtensionCommandsArgs {
  context: vscode.ExtensionContext;
  state: ActivationState;
  provider: TicketTreeProvider;
  outputChannel: vscode.OutputChannel;
  statusBarItem: vscode.StatusBarItem;
  updateStatusBar: () => void;
  getServerProcess: () => ChildProcess | undefined;
  setServerProcess: (process: ChildProcess | undefined) => void;
}

export function registerExtensionCommands(args: RegisterExtensionCommandsArgs): void {
  const { context, state, provider, outputChannel, statusBarItem, updateStatusBar, getServerProcess, setServerProcess } = args;

  async function runMutation(action: () => Promise<unknown>): Promise<void> {
    try {
      await action();
      provider.refresh();
    } catch (err) {
      void vscode.window.showErrorMessage(
        err instanceof Error ? err.message : String(err),
      );
    }
  }

  function announceFilterChange(): void {
    const summary = provider.filterSummary;
    vscode.window.setStatusBarMessage(
      summary ? `Ticket filters: ${summary}` : 'Ticket filters cleared',
      4000,
    );
  }

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.openBrowser', () => {
      openTicketViewer(state.serverUrl);
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.refresh', () => {
      provider.refresh();
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.setSearchQuery', () => {
      const inputBox = vscode.window.createInputBox();
      inputBox.title = 'Search Tickets';
      inputBox.value = provider.filters.query ?? '';
      inputBox.placeholder = 'Type to filter tickets in real time…';
      inputBox.prompt = 'Enter to fetch from server · Escape to cancel';

      let accepted = false;

      inputBox.onDidChangeValue(value => {
        provider.setLocalSearch(value);
      });

      inputBox.onDidAccept(() => {
        accepted = true;
        const value = inputBox.value;
        inputBox.hide();
        provider.setLocalSearch('');
        provider.setSearchQuery(value);
        announceFilterChange();
      });

      inputBox.onDidHide(() => {
        inputBox.dispose();
        if (!accepted) {
          provider.setLocalSearch('');
        }
      });

      inputBox.show();
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.setStateFilter', async () => {
      const current = provider.filters.state;
      const states = provider.availableStates.length > 0
        ? provider.availableStates
        : TICKET_STATES;
      const picks = [
        {
          label: 'All states',
          description: current ? 'Clear active state filter' : 'No active state filter',
          state: undefined as string | undefined,
        },
        ...states.map(ticketState => ({
          label: ticketState,
          description: ticketState === current ? '← current' : undefined,
          state: ticketState,
        })),
      ];
      const pick = await vscode.window.showQuickPick(picks, {
        title: 'Filter Tickets By State',
        placeHolder: current ?? 'All states',
      });
      if (!pick) { return; }
      provider.setStateFilter(pick.state);
      announceFilterChange();
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.clearFilters', () => {
      provider.clearFilters();
      announceFilterChange();
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.startServer', async () => {
      if (await pingServer(state.serverUrl)) {
        provider.refresh();
        vscode.window.setStatusBarMessage(`$(server) Server already running at ${state.serverUrl}`, 3000);
        return;
      }
      const serverProcess = getServerProcess();
      if (serverProcess && !serverProcess.killed) {
        serverProcess.kill();
      }
      try {
        const handle = await startServerTask(outputChannel, state.config);
        setServerProcess(handle.process);
        state.serverUrl = handle.serverUrl;
        vscode.window.setStatusBarMessage(`$(server) Ticket server running on ${state.serverUrl}`, 5000);
        provider.update(
          state.serverUrl,
          state.workspace,
          state.config.autoRefreshSeconds,
          resolveTicketsDir(state.workspace),
        );
        statusBarItem.tooltip = `Open Ticket Viewer (${state.serverUrl})`;
        void pollUntilReachable(state.serverUrl, 30_000).then(() => provider.refresh());
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        void vscode.window.showErrorMessage(`Failed to start server: ${msg}`);
      }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.openTicket', (item: TicketItem) => {
      const ticketUrl = `${state.serverUrl}/workspace/${encodeURIComponent(state.workspace)}/ticket/${encodeURIComponent(item.ticket.id)}`;
      openTicketViewer(ticketUrl);
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.copyId', (item: TicketItem) => {
      void vscode.env.clipboard.writeText(item.ticket.id).then(() => {
        vscode.window.setStatusBarMessage(`$(check) Copied: ${item.ticket.id}`, 5000);
      });
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.openInTicketViewer', (item: TicketItem) => {
      const ticketUrl = `${state.serverUrl}/workspace/${encodeURIComponent(state.workspace)}/ticket/${encodeURIComponent(item.ticket.id)}`;
      openTicketViewer(ticketUrl);
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.createTicket', async () => {
      const typeItems = TICKET_TYPES.map(type => ({ label: type }));
      const typePick = await vscode.window.showQuickPick(typeItems, {
        title: 'New Ticket — Select Type',
        placeHolder: 'Ticket type',
      });
      if (!typePick) { return; }

      const title = await vscode.window.showInputBox({
        title: 'New Ticket — Title',
        prompt: 'Enter a title for the new ticket',
        ignoreFocusOut: true,
      });
      if (title === undefined) { return; }

      await runMutation(() => createTicket(state.serverUrl, state.workspace, typePick.label, title));
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.editTitle', async (item: TicketItem) => {
      const newTitle = await vscode.window.showInputBox({
        title: 'Edit Title',
        value: item.ticket.title ?? '',
        prompt: 'New title for the ticket',
        ignoreFocusOut: true,
      });
      if (newTitle === undefined) { return; }
      await runMutation(() =>
        updateTicket(state.serverUrl, state.workspace, item.ticket.id, {
          fields: { title: newTitle },
        }),
      );
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.setState', async (item: TicketItem) => {
      const current = item.ticket.state;
      const stateItems = TICKET_STATES.map(ticketState => ({
        label: ticketState,
        description: ticketState === current ? '← current' : undefined,
      }));
      const pick = await vscode.window.showQuickPick(stateItems, {
        title: `Set State — ${item.ticket.title ?? item.ticket.id.slice(0, 8)}`,
        placeHolder: `Current: ${current ?? 'unknown'}`,
      });
      if (!pick || pick.label === current) { return; }
      await runMutation(() =>
        updateTicket(state.serverUrl, state.workspace, item.ticket.id, { state: pick.label }),
      );
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.editDescription', (item: TicketItem) => {
      const ticketsDir = resolveTicketsDir(state.workspace);
      if (!ticketsDir) {
        void vscode.window.showWarningMessage('Ticket folder not found on disk.');
        return;
      }
      const descPath = path.join(ticketsDir, item.ticket.id, 'description.md');
      void vscode.commands.executeCommand('vscode.open', vscode.Uri.file(descPath));
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.previewDescription', (ticketId: string) => {
      const ticketsDir = resolveTicketsDir(state.workspace);
      if (!ticketsDir) {
        void vscode.window.showWarningMessage('Ticket folder not found on disk.');
        return;
      }
      const descPath = path.join(ticketsDir, ticketId, 'description.md');
      if (!fs.existsSync(descPath)) {
        void vscode.window.showInformationMessage('No description.md found for this ticket.');
        return;
      }
      void vscode.commands.executeCommand('markdown.showPreviewToSide', vscode.Uri.file(descPath));
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.closeTicket', async (item: TicketItem) => {
      const confirm = await vscode.window.showWarningMessage(
        `Fast-forward "${item.ticket.title ?? item.ticket.id.slice(0, 8)}" to done?`,
        { modal: true }, 'Close Ticket',
      );
      if (confirm !== 'Close Ticket') { return; }
      await runMutation(() => closeTicket(state.serverUrl, state.workspace, item.ticket.id));
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.cancelTicket', async (item: TicketItem) => {
      const reason = await vscode.window.showInputBox({
        title: `Cancel — ${item.ticket.title ?? item.ticket.id.slice(0, 8)}`,
        prompt: 'Reason for cancellation (optional)',
        ignoreFocusOut: true,
      });
      if (reason === undefined) { return; }
      await runMutation(() =>
        cancelTicket(state.serverUrl, state.workspace, item.ticket.id, reason || undefined),
      );
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.undoTicket', async (item: TicketItem) => {
      const confirm = await vscode.window.showWarningMessage(
        `Undo last transition on "${item.ticket.title ?? item.ticket.id.slice(0, 8)}"?`,
        { modal: true }, 'Undo',
      );
      if (confirm !== 'Undo') { return; }
      await runMutation(() => undoTicket(state.serverUrl, state.workspace, item.ticket.id));
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.addDependency', async (item: TicketItem) => {
      const all = provider.allTickets.filter(ticket => ticket.id !== item.ticket.id);
      const picks = all.map(ticket => ({
        label: ticket.title ?? `(${ticket.id.slice(0, 8)})`,
        description: `${ticket.id.slice(0, 8)} · ${ticket.state ?? '?'}`,
        id: ticket.id,
      }));
      const pick = await vscode.window.showQuickPick(picks, {
        title: `Add Dependency to "${item.ticket.title ?? item.ticket.id.slice(0, 8)}"`,
        placeHolder: 'Select dependency ticket',
      });
      if (!pick) { return; }
      await runMutation(() =>
        addEdge(state.serverUrl, state.workspace, item.ticket.id, pick.id, 'depends_on'),
      );
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.deleteTicket', async (item: TicketItem) => {
      const confirm = await vscode.window.showWarningMessage(
        `Delete "${item.ticket.title ?? item.ticket.id.slice(0, 8)}"? This cannot be undone.`,
        { modal: true }, 'Delete',
      );
      if (confirm !== 'Delete') { return; }
      await runMutation(() => deleteTicket(state.serverUrl, state.workspace, item.ticket.id));
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.selectWorkspace', async () => {
      const detected = detectTicketWorkspaces();
      if (detected.length === 0) {
        void vscode.window.showInformationMessage(
          'No .ticket workspaces found in the currently open folders.',
        );
        return;
      }
      const items = detected.map(candidate => ({
        label: candidate.folderName,
        description: candidate.ticketPath,
        folderName: candidate.folderName,
      }));
      const pick = await vscode.window.showQuickPick(items, {
        placeHolder: 'Select active ticket workspace',
        title: 'Active Ticket Workspace',
      });
      if (!pick) { return; }
      await context.workspaceState.update('activeTicketFolder', pick.folderName);
      const resolved = await resolveActiveWorkspace(state.serverUrl, state.config.workspace, context);
      state.workspace = resolved.workspace;
      state.displayName = resolved.displayName;
      provider.update(
        state.serverUrl,
        state.workspace,
        state.config.autoRefreshSeconds,
        resolveTicketsDir(state.workspace),
      );
      updateStatusBar();
    }),
  );

  const bridge = new BrowserBridge({
    controlPort: state.config.bridgePort,
    cdpPort: state.config.cdpPort,
    autoConnectCdp: state.config.autoConnectCdp,
  });
  context.subscriptions.push(bridge);

  bridge.start().catch(err => {
    const msg = err instanceof Error ? err.message : String(err);
    void vscode.window.showWarningMessage(`Browser Bridge failed to start: ${msg}`);
  });

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.bridgeNavigate', async () => {
      const url = await vscode.window.showInputBox({
        prompt: 'URL to open in Simple Browser',
        value: state.serverUrl,
      });
      if (url) { await bridge.navigate(url); }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.bridgeConnectCdp', async () => {
      const ok = await bridge.connectCdp();
      if (ok) {
        void vscode.window.showInformationMessage('Browser Bridge: CDP connected.');
      }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.bridgeStatus', () => {
      const bridgeState = bridge.state;
      void vscode.window.showInformationMessage(
        `Bridge port: ${bridgeState.controlPort} | CDP: ${bridgeState.cdpConnected ? 'connected' : 'disconnected'} | URL: ${bridgeState.currentUrl ?? 'none'}`,
      );
    }),
  );

  context.subscriptions.push(
    vscode.workspace.onDidChangeWorkspaceFolders(async () => {
      const resolved = await resolveActiveWorkspace(state.serverUrl, state.config.workspace, context);
      state.workspace = resolved.workspace;
      state.displayName = resolved.displayName;
      provider.update(
        state.serverUrl,
        state.workspace,
        state.config.autoRefreshSeconds,
        resolveTicketsDir(state.workspace),
      );
      updateStatusBar();
    }),
  );

  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async event => {
      if (!event.affectsConfiguration('ticketViewer')) { return; }
      state.config = readConfig();
      if (!state.config.autoStartServer) {
        state.serverUrl = state.config.serverUrl;
      }
      const resolved = await resolveActiveWorkspace(state.serverUrl, state.config.workspace, context);
      state.workspace = resolved.workspace;
      state.displayName = resolved.displayName;
      provider.update(
        state.serverUrl,
        state.workspace,
        state.config.autoRefreshSeconds,
        resolveTicketsDir(state.workspace),
      );
      statusBarItem.tooltip = `Open Ticket Viewer (${state.serverUrl})`;
      updateStatusBar();
    }),
  );
}