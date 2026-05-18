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
  openTicketViewer: jest.fn(),
  pingServer: jest.fn(),
  pollUntilReachable: jest.fn(() => Promise.resolve()),
  readConfig: jest.fn(),
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
  } as never;

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

  return { registered };
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
});