import * as fs from 'node:fs';
import * as path from 'node:path';
import { EventEmitter } from 'node:events';
import * as vscode from 'vscode';
jest.mock('node:child_process', () => {
  const actual = jest.requireActual('node:child_process');
  return {
    ...actual,
    execFile: jest.fn(),
    spawn: jest.fn(),
  };
});
jest.mock('../../src/api', () => ({
  fetchWorkspaces: jest.fn(),
}));
jest.mock('node:fs', () => ({
  existsSync: jest.fn(),
  statSync: jest.fn(),
}));
import { fetchWorkspaces } from '../../src/api';
import {
  buildServerDiscoveryCandidates,
  discoverRunningServerUrl,
  parseListeningPorts,
  resolveActiveWorkspace,
  resolveServerLaunch,
  startServerTask,
  resolveTicketsDir,
  type TicketViewerConfig,
} from '../../src/extensionSupport';
import { execFile, spawn } from 'node:child_process';

const mockFs = fs as jest.Mocked<typeof fs>;
const mockWorkspace = vscode.workspace as any;
const mockFetchWorkspaces = fetchWorkspaces as jest.MockedFunction<typeof fetchWorkspaces>;
const mockExecFile = execFile as jest.MockedFunction<typeof execFile>;
const mockSpawn = spawn as jest.MockedFunction<typeof spawn>;

function makeConfig(overrides: Partial<TicketViewerConfig> = {}): TicketViewerConfig {
  return {
    serverUrl: 'http://localhost:3002',
    workspace: '',
    autoRefreshSeconds: 30,
    autoStartServer: true,
    bridgePort: 0,
    cdpPort: 0,
    autoConnectCdp: true,
    serverBinaryPath: '',
    serverWorkingDirectory: '',
    ...overrides,
  };
}

function directoryStat(): fs.Stats {
  return {
    isDirectory: () => true,
    isFile: () => false,
    mode: 0o755,
  } as fs.Stats;
}

function fileStat(mode = 0o755): fs.Stats {
  return {
    isDirectory: () => false,
    isFile: () => true,
    mode,
  } as fs.Stats;
}

describe('resolveServerLaunch', () => {
  const originalPath = process.env.PATH;
  const originalWorkspaceFolders = mockWorkspace.workspaceFolders;
  const binaryName = process.platform === 'win32' ? 'ticket-viewer.exe' : 'ticket-viewer';
  const workspaceRoot = path.join(path.sep, 'repo', 'workspace');
  const ticketDir = path.join(workspaceRoot, '.ticket');
  const debugBinary = path.join(workspaceRoot, 'target', 'debug', binaryName);
  const pathDir = path.join(path.sep, 'tools', 'bin');
  const pathBinary = path.join(pathDir, binaryName);

  beforeEach(() => {
    mockFetchWorkspaces.mockReset();
    mockExecFile.mockReset();
    mockSpawn.mockReset();
    mockFs.existsSync.mockReset();
    mockFs.statSync.mockReset();
    mockWorkspace.workspaceFolders = [
      {
        index: 0,
        name: 'workspace',
        uri: vscode.Uri.file(workspaceRoot),
      },
    ];
    mockFs.existsSync.mockReturnValue(false);
  });

  afterEach(() => {
    process.env.PATH = originalPath;
    mockWorkspace.workspaceFolders = originalWorkspaceFolders;
  });

  test('prefers explicit serverBinaryPath over PATH and debug binaries', () => {
    const explicitBinary = path.join(path.sep, 'custom', binaryName);
    process.env.PATH = pathDir;
    mockFs.existsSync.mockImplementation(candidate => candidate.toString() === debugBinary);
    mockFs.statSync.mockImplementation(candidate => {
      const candidatePath = candidate.toString();
      if (candidatePath === ticketDir) {
        return directoryStat();
      }
      if (candidatePath === pathBinary) {
        return fileStat();
      }
      throw new Error(`ENOENT: ${candidatePath}`);
    });

    const resolved = resolveServerLaunch(makeConfig({ serverBinaryPath: explicitBinary }));

    expect(resolved.cmd).toBe(explicitBinary);
    expect(resolved.args).toEqual(['--index-root', ticketDir]);
  });

  test('prefers PATH binary before the workspace debug binary', () => {
    process.env.PATH = pathDir;
    mockFs.existsSync.mockImplementation(candidate => candidate.toString() === debugBinary);
    mockFs.statSync.mockImplementation(candidate => {
      const candidatePath = candidate.toString();
      if (candidatePath === ticketDir) {
        return directoryStat();
      }
      if (candidatePath === pathBinary) {
        return fileStat();
      }
      throw new Error(`ENOENT: ${candidatePath}`);
    });

    const resolved = resolveServerLaunch(makeConfig());

    expect(resolved.cmd).toBe(pathBinary);
    expect(resolved.args).toEqual(['--index-root', ticketDir]);
    expect(resolved.cwd).toBe(workspaceRoot);
  });

  test('falls back to the workspace debug binary when PATH has no ticket-viewer', () => {
    process.env.PATH = pathDir;
    mockFs.existsSync.mockImplementation(candidate => candidate.toString() === debugBinary);
    mockFs.statSync.mockImplementation(candidate => {
      const candidatePath = candidate.toString();
      if (candidatePath === ticketDir) {
        return directoryStat();
      }
      throw new Error(`ENOENT: ${candidatePath}`);
    });

    const resolved = resolveServerLaunch(makeConfig());

    expect(resolved.cmd).toBe(debugBinary);
    expect(resolved.args).toEqual(['--index-root', ticketDir]);
    expect(resolved.cwd).toBe(workspaceRoot);
  });

  test('uses workspace-root cwd and omits --index-root when no .ticket workspace is detected', () => {
    process.env.PATH = '';
    mockFs.statSync.mockImplementation(candidate => {
      const candidatePath = candidate.toString();
      if (candidatePath === ticketDir) {
        throw new Error(`ENOENT: ${candidatePath}`);
      }
      throw new Error(`ENOENT: ${candidatePath}`);
    });
    mockFs.existsSync.mockReturnValue(false);

    const resolved = resolveServerLaunch(makeConfig());

    expect(resolved.args).toEqual([]);
    expect(resolved.cwd).toBe(workspaceRoot);
    expect(resolved.args).not.toContain('--index-root');
    expect(mockFs.statSync).toHaveBeenCalledWith(ticketDir);

    const inspectedPaths = mockFs.statSync.mock.calls.map(call => call[0].toString());
    expect(inspectedPaths.some(p => p.endsWith(`${path.sep}.spec`))).toBe(false);
    expect(inspectedPaths.some(p => p.endsWith(`${path.sep}.rule`))).toBe(false);
  });

  test('startServerTask reports controlled startup failure without --index-root when no .ticket is detected', async () => {
    process.env.PATH = '';
    mockFs.statSync.mockImplementation(candidate => {
      const candidatePath = candidate.toString();
      throw new Error(`ENOENT: ${candidatePath}`);
    });
    mockFs.existsSync.mockReturnValue(false);

    const proc = new EventEmitter() as any;
    proc.stdout = new EventEmitter();
    proc.stderr = new EventEmitter();
    proc.killed = false;
    mockSpawn.mockImplementation((() => proc) as unknown as typeof spawn);

    const setTimeoutSpy = jest
      .spyOn(global, 'setTimeout')
      .mockImplementation((() => 0) as unknown as typeof setTimeout);

    const outputChannel = {
      appendLine: jest.fn(),
      append: jest.fn(),
    } as unknown as vscode.OutputChannel;

    const startup = startServerTask(outputChannel, makeConfig());
    proc.emit('error', new Error('spawn ENOENT'));
    await expect(startup).rejects.toThrow('spawn ENOENT');

    const spawnArgs = mockSpawn.mock.calls[0][1] as string[];
    expect(spawnArgs).toEqual(expect.arrayContaining(['--port', '0']));
    expect(spawnArgs).not.toContain('--index-root');
    expect(outputChannel.appendLine).toHaveBeenCalledWith(
      `[ticket-viewer] Working directory: ${workspaceRoot}`,
    );

    setTimeoutSpy.mockRestore();
  });

  test('prefers the label-matched canonical workspace id over the first server workspace', async () => {
    mockFs.statSync.mockImplementation(candidate => {
      if (candidate.toString() === ticketDir) {
        return directoryStat();
      }
      throw new Error(`ENOENT: ${candidate.toString()}`);
    });
    mockFetchWorkspaces.mockResolvedValue({
      request_id: 'req-1',
      workspaces: [
        {
          name: 'C:/Users/linus/git/graph_app/context-engine/context-stack/tools/context-editor/.ticket',
          label: 'context-editor',
        },
        {
          name: 'default',
          label: 'workspace',
        },
      ],
    });

    const resolved = await resolveActiveWorkspace(
      'http://localhost:3002',
      '',
      {
        workspaceState: {
          get: jest.fn(),
          update: jest.fn(),
        },
      } as unknown as vscode.ExtensionContext,
    );

    expect(resolved).toEqual({ workspace: 'default', displayName: 'workspace' });
  });

  test('falls back to the server active workspace when folder-name matching misses', async () => {
    mockFs.statSync.mockImplementation(candidate => {
      if (candidate.toString() === ticketDir) {
        return directoryStat();
      }
      throw new Error(`ENOENT: ${candidate.toString()}`);
    });
    mockFetchWorkspaces.mockResolvedValue({
      request_id: 'req-2',
      active_workspace: 'default',
      workspaces: [
        {
          name: 'C:/Users/linus/git/graph_app/context-engine/context-stack/tools/context-editor/.ticket',
          label: 'context-editor',
        },
        {
          name: 'default',
          label: 'context-engine',
        },
      ],
    });

    const resolved = await resolveActiveWorkspace(
      'http://localhost:3002',
      '',
      {
        workspaceState: {
          get: jest.fn(),
          update: jest.fn(),
        },
      } as unknown as vscode.ExtensionContext,
    );

    expect(resolved).toEqual({ workspace: 'default', displayName: 'workspace' });
  });

  test('ignores a stale configured workspace id when the server exposes canonical workspace names', async () => {
    mockFs.statSync.mockImplementation(candidate => {
      if (candidate.toString() === ticketDir) {
        return directoryStat();
      }
      throw new Error(`ENOENT: ${candidate.toString()}`);
    });
    mockFetchWorkspaces.mockResolvedValue({
      request_id: 'req-3',
      active_workspace: 'context-engine--9bce5444',
      workspaces: [
        {
          name: 'context-editor--b0870155',
          label: 'context-editor',
        },
        {
          name: 'context-engine--9bce5444',
          label: 'workspace',
        },
      ],
    });

    const resolved = await resolveActiveWorkspace(
      'http://localhost:56882',
      'default',
      {
        workspaceState: {
          get: jest.fn(),
          update: jest.fn(),
        },
      } as unknown as vscode.ExtensionContext,
    );

    expect(resolved).toEqual({
      workspace: 'context-engine--9bce5444',
      displayName: 'workspace',
    });
  });

  test('resolves the local tickets directory from display name when workspace ids are canonical', () => {
    mockFs.statSync.mockImplementation(candidate => {
      const candidatePath = candidate.toString();
      if (candidatePath === ticketDir || candidatePath === path.join(ticketDir, 'tickets')) {
        return directoryStat();
      }
      throw new Error(`ENOENT: ${candidatePath}`);
    });

    expect(resolveTicketsDir('shared--abc123', 'workspace')).toBe(path.join(ticketDir, 'tickets'));
  });

  test('parseListeningPorts extracts localhost listen ports across command formats', () => {
    const output = [
      'TCP    127.0.0.1:55838    0.0.0.0:0    LISTENING    1234',
      'LISTEN 0 128 127.0.0.1:3002 0.0.0.0:*',
      'node 123 user 10u IPv4 0x0 TCP *:4002 (LISTEN)',
    ].join('\n');

    expect(parseListeningPorts(output)).toEqual([55838, 3002, 4002]);
  });

  test('buildServerDiscoveryCandidates prefers preferred, stored, configured, then scanned localhost ports', () => {
    expect(buildServerDiscoveryCandidates(
      'http://localhost:3002',
      'http://localhost:55838',
      [3002, 60123],
      ['http://localhost:4002'],
    )).toEqual([
      'http://localhost:4002',
      'http://localhost:55838',
      'http://localhost:3002',
      'http://localhost:60123',
    ]);
  });

  test('discoverRunningServerUrl probes local listeners and stores the first reachable ticket server', async () => {
    mockExecFile.mockImplementation((...args: any[]) => {
      const callback = args[args.length - 1];
      callback(null, 'TCP    127.0.0.1:55838    0.0.0.0:0    LISTENING    1234\n', '');
      return {} as any;
    });

    const fetchMock = jest.fn()
      .mockResolvedValueOnce({ ok: true });
    (globalThis as any).fetch = fetchMock;

    const update = jest.fn().mockResolvedValue(undefined);
    const context = {
      workspaceState: {
        get: jest.fn().mockReturnValue(undefined),
        update,
      },
    } as unknown as vscode.ExtensionContext;

    const discovered = await discoverRunningServerUrl(
      context,
      makeConfig(),
      undefined,
      ['http://localhost:55838'],
    );

    expect(discovered).toBe('http://localhost:55838');
    expect(fetchMock).toHaveBeenCalledWith(
      'http://localhost:55838/api/workspaces',
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
    expect(update).toHaveBeenCalledWith('ticketViewer.lastServerUrl', 'http://localhost:55838');
  });
});
