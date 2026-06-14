import * as vscode from 'vscode';

jest.mock('../../src/ticketProvider', () => {
  const instances: any[] = [];

  const TicketTreeProvider = jest.fn().mockImplementation(
    (baseUrl: string, workspace: string, autoRefreshSeconds: number, ticketsDir?: string) => {
      const provider = {
        allTickets: [],
        filterSummary: undefined,
        refresh: jest.fn(),
        update: jest.fn(),
        onDidChangeTreeData: jest.fn(() => ({ dispose: () => {} })),
        dispose: jest.fn(),
        _ctor: { baseUrl, workspace, autoRefreshSeconds, ticketsDir },
      };
      instances.push(provider);
      return provider;
    },
  );

  return {
    TicketTreeProvider,
    __providerInstances: instances,
  };
});

jest.mock('../../src/extensionCommands', () => ({
  registerExtensionCommands: jest.fn(),
}));

jest.mock('../../src/extensionSupport', () => ({
  pingServer: jest.fn(),
  pollUntilReachable: jest.fn(() => Promise.resolve()),
  readConfig: jest.fn(),
  resolveActiveWorkspace: jest.fn(),
  resolveTicketsDir: jest.fn(() => 'C:/tickets'),
  startServerTask: jest.fn(),
}));

import { activate, deactivate } from '../../src/extension';
import { registerExtensionCommands } from '../../src/extensionCommands';
import * as extensionSupport from '../../src/extensionSupport';

type ProviderMock = {
  update: jest.Mock;
  refresh: jest.Mock;
  onDidChangeTreeData: jest.Mock;
  _ctor: {
    baseUrl: string;
    workspace: string;
    autoRefreshSeconds: number;
    ticketsDir?: string;
  };
};

function getProviderInstances(): ProviderMock[] {
  return (jest.requireMock('../../src/ticketProvider') as { __providerInstances: ProviderMock[] }).__providerInstances;
}

function makeContext(): vscode.ExtensionContext {
  return {
    subscriptions: [],
    workspaceState: {
      get: jest.fn(),
      update: jest.fn().mockResolvedValue(undefined),
    },
  } as unknown as vscode.ExtensionContext;
}

describe('extension activation', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    getProviderInstances().length = 0;
  });

  afterEach(() => {
    deactivate();
  });

  test('re-resolves the workspace after an auto-started server becomes reachable', async () => {
    const processMock = {
      killed: false,
      kill: jest.fn(function kill(this: { killed: boolean }) {
        this.killed = true;
      }),
    };

    (extensionSupport.readConfig as jest.Mock).mockReturnValue({
      autoConnectCdp: false,
      autoRefreshSeconds: 30,
      autoStartServer: true,
      bridgePort: 0,
      cdpPort: 0,
      serverBinaryPath: '',
      serverUrl: 'http://localhost:3002',
      serverWorkingDirectory: '',
      workspace: '',
    });
    (extensionSupport.pingServer as jest.Mock).mockResolvedValue(false);
    (extensionSupport.startServerTask as jest.Mock).mockResolvedValue({
      process: processMock,
      serverUrl: 'http://localhost:55838',
    });
    (extensionSupport.resolveActiveWorkspace as jest.Mock)
      .mockResolvedValueOnce({ workspace: 'default', displayName: 'workspace' })
      .mockResolvedValueOnce({ workspace: 'shared--abc123', displayName: 'workspace' });

    const context = makeContext();

    await activate(context);
    await Promise.resolve();
    await Promise.resolve();

    const [provider] = getProviderInstances();

    expect(provider._ctor).toEqual({
      baseUrl: 'http://localhost:55838',
      workspace: 'default',
      autoRefreshSeconds: 30,
      ticketsDir: 'C:/tickets',
    });
    expect(extensionSupport.pollUntilReachable).toHaveBeenCalledTimes(1);
    expect(extensionSupport.pollUntilReachable).toHaveBeenCalledWith('http://localhost:55838', 30_000);
    expect(extensionSupport.resolveActiveWorkspace).toHaveBeenNthCalledWith(2, 'http://localhost:55838', '', context);
    expect(extensionSupport.resolveTicketsDir).toHaveBeenLastCalledWith('shared--abc123', 'workspace');
    expect(provider.update).toHaveBeenCalledWith(
      'http://localhost:55838',
      'shared--abc123',
      30,
      'C:/tickets',
    );
    expect(registerExtensionCommands).toHaveBeenCalledTimes(1);
  });
});