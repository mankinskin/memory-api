import * as fs from 'node:fs';
import * as path from 'node:path';
import * as vscode from 'vscode';

jest.mock('../../src/api', () => ({
  fetchWorkspaces: jest.fn(),
}));

jest.mock('node:fs', () => ({
  existsSync: jest.fn(),
  statSync: jest.fn(),
}));

import { fetchWorkspaces } from '../../src/api';
import {
  resolveActiveWorkspace,
  resolveServerLaunch,
  resolveTicketsDir,
  type TicketViewerConfig,
} from '../../src/extensionSupport';

const mockFs = fs as jest.Mocked<typeof fs>;
const mockWorkspace = vscode.workspace as any;
const mockFetchWorkspaces = fetchWorkspaces as jest.MockedFunction<typeof fetchWorkspaces>;

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
});