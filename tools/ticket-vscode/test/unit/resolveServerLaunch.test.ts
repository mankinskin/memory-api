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

import { resolveServerLaunch, type TicketViewerConfig } from '../../src/extensionSupport';

const mockFs = fs as jest.Mocked<typeof fs>;
const mockWorkspace = vscode.workspace as any;

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
});