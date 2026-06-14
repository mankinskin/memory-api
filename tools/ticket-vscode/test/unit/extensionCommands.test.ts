import * as fs from 'node:fs';
import * as vscode from 'vscode';

import { registerExtensionCommands, type ActivationState } from '../../src/extensionCommands';

jest.mock('../../src/api', () => ({
  addEdge: jest.fn(),
  cancelTicket: jest.fn(),
  closeTicket: jest.fn(),
  createTicket: jest.fn(),
  deleteTicket: jest.fn(),
  undoTicket: jest.fn(),
  updateTicket: jest.fn(),
}));

jest.mock('../../src/browserBridge', () => ({
  BrowserBridge: jest.fn().mockImplementation(() => ({
    start: jest.fn().mockResolvedValue(undefined),
    navigate: jest.fn().mockResolvedValue(undefined),
    connectCdp: jest.fn().mockResolvedValue(false),
    dispose: jest.fn(),
    state: {
      controlPort: 0,
      cdpConnected: false,
      currentUrl: null,
    },
  })),
}));

jest.mock('../../src/extensionSupport', () => ({
  TICKET_STATES: ['new', 'ready', 'in-implementation', 'in-review', 'done', 'cancelled'],
  TICKET_TYPES: ['tracker-improvement'],
  detectTicketWorkspaces: jest.fn(() => []),
  discoverRunningServerUrl: jest.fn(),
  openTicketViewer: jest.fn(),
  pingServer: jest.fn(),
  pollUntilReachable: jest.fn(() => Promise.resolve()),
  readConfig: jest.fn(),
  rememberServerUrl: jest.fn(() => Promise.resolve()),
  resolveActiveWorkspace: jest.fn(),
  resolveTicketsDir: jest.fn(() => 'C:/tickets'),
  startServerTask: jest.fn(),
}));

jest.mock('node:fs', () => ({
  existsSync: jest.fn(() => true),
}));

import * as extensionSupport from '../../src/extensionSupport';

type RegisteredCommand = (...args: unknown[]) => unknown;

function createArgs() {
  const subscriptions: Array<{ dispose: () => void }> = [];
  const registered = new Map<string, RegisteredCommand>();

  (vscode.commands.registerCommand as jest.Mock).mockImplementation(
    (command: string, callback: RegisteredCommand) => {
      registered.set(command, callback);
      return { dispose: () => {} };
    },
  );

  const context = {
    subscriptions,
    workspaceState: {
      update: jest.fn().mockResolvedValue(undefined),
    },
  } as unknown as vscode.ExtensionContext;

  const state: ActivationState = {
    config: {
      autoConnectCdp: false,
      autoRefreshSeconds: 0,
      autoStartServer: false,
      bridgePort: 0,
      cdpPort: 9222,
      serverBinaryPath: '',
      serverUrl: 'http://localhost:3002',
      serverWorkingDirectory: '',
      workspace: 'default',
    },
    serverUrl: 'http://localhost:3002',
    workspace: 'default',
    displayName: 'default',
  };

  const provider = {
    allTickets: [],
    availableStates: [],
    filterSummary: undefined,
    filters: {},
    refresh: jest.fn(),
    update: jest.fn(),
    setLocalSearch: jest.fn(),
    setSearchQuery: jest.fn(),
    setStateFilter: jest.fn(),
    clearFilters: jest.fn(),
  } as any;

  registerExtensionCommands({
    context,
    state,
    provider,
    outputChannel: { appendLine: jest.fn() } as unknown as vscode.OutputChannel,
    statusBarItem: { tooltip: '' } as unknown as vscode.StatusBarItem,
    updateStatusBar: jest.fn(),
    getServerProcess: () => undefined,
    setServerProcess: jest.fn(),
  });

  return { registered, provider, state, context };
}

describe('registerExtensionCommands', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  test('ticket-viewer.openTicket opens the selected ticket URL even when description.md exists', () => {
    const { registered } = createArgs();
    const openTicket = registered.get('ticket-viewer.openTicket');

    expect(openTicket).toBeDefined();

    openTicket?.({
      ticket: {
        id: 'ticket id/with spaces',
        title: 'Example ticket',
      },
    });

    expect(extensionSupport.openTicketViewer).toHaveBeenCalledWith(
      'http://localhost:3002/workspace/default/ticket/ticket%20id%2Fwith%20spaces',
    );
    expect(extensionSupport.resolveTicketsDir).not.toHaveBeenCalled();
    expect(fs.existsSync).not.toHaveBeenCalled();
    expect(vscode.commands.executeCommand).not.toHaveBeenCalledWith(
      'markdown.showPreviewToSide',
      expect.anything(),
    );
  });

  test('ticket-viewer.startServer re-resolves the workspace when the server is already reachable', async () => {
    const { registered, provider, state, context } = createArgs();
    const startServer = registered.get('ticket-viewer.startServer');

    (extensionSupport.discoverRunningServerUrl as jest.Mock).mockResolvedValue(undefined);
    (extensionSupport.pingServer as jest.Mock).mockResolvedValue(true);
    (extensionSupport.resolveActiveWorkspace as jest.Mock).mockResolvedValue({
      workspace: 'shared--abc123',
      displayName: 'workspace',
    });

    await startServer?.();

    expect(extensionSupport.resolveActiveWorkspace).toHaveBeenCalledWith(
      'http://localhost:3002',
      'default',
      context,
    );
    expect(extensionSupport.resolveTicketsDir).toHaveBeenCalledWith('shared--abc123', 'workspace');
    expect(provider.update).toHaveBeenCalledWith(
      'http://localhost:3002',
      'shared--abc123',
      0,
      'C:/tickets',
    );
    expect(state.workspace).toBe('shared--abc123');
    expect(state.displayName).toBe('workspace');
  });

  test('ticket-viewer.startServer uses a discovered running server before trying to spawn', async () => {
    const { registered, provider, state, context } = createArgs();
    const startServer = registered.get('ticket-viewer.startServer');

    (extensionSupport.discoverRunningServerUrl as jest.Mock).mockResolvedValue('http://localhost:55838');
    (extensionSupport.resolveActiveWorkspace as jest.Mock).mockResolvedValue({
      workspace: 'shared--abc123',
      displayName: 'workspace',
    });

    await startServer?.();

    expect(extensionSupport.startServerTask).not.toHaveBeenCalled();
    expect(extensionSupport.pingServer).not.toHaveBeenCalled();
    expect(extensionSupport.resolveActiveWorkspace).toHaveBeenCalledWith(
      'http://localhost:55838',
      'default',
      context,
    );
    expect(provider.update).toHaveBeenCalledWith(
      'http://localhost:55838',
      'shared--abc123',
      0,
      'C:/tickets',
    );
    expect(state.serverUrl).toBe('http://localhost:55838');
  });

  test('ticket-viewer.startServer re-resolves the workspace after a spawned server becomes reachable', async () => {
    const { registered, provider, state, context } = createArgs();
    const startServer = registered.get('ticket-viewer.startServer');

    (extensionSupport.discoverRunningServerUrl as jest.Mock).mockResolvedValue(undefined);
    (extensionSupport.pingServer as jest.Mock).mockResolvedValue(false);
    (extensionSupport.startServerTask as jest.Mock).mockResolvedValue({
      process: { killed: false },
      serverUrl: 'http://localhost:55838',
    });
    (extensionSupport.pollUntilReachable as jest.Mock).mockResolvedValue(undefined);
    (extensionSupport.resolveActiveWorkspace as jest.Mock).mockResolvedValue({
      workspace: 'shared--abc123',
      displayName: 'workspace',
    });

    await startServer?.();
    await Promise.resolve();

    expect(extensionSupport.pollUntilReachable).toHaveBeenCalledWith('http://localhost:55838', 30_000);
    expect(extensionSupport.resolveActiveWorkspace).toHaveBeenCalledWith(
      'http://localhost:55838',
      'default',
      context,
    );
    expect(extensionSupport.resolveTicketsDir).toHaveBeenCalledWith('shared--abc123', 'workspace');
    expect(provider.update).toHaveBeenCalledWith(
      'http://localhost:55838',
      'shared--abc123',
      0,
      'C:/tickets',
    );
    expect(state.serverUrl).toBe('http://localhost:55838');
  });
});