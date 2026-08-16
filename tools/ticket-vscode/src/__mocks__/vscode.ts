// Minimal vscode mock for unit testing ticketProvider logic.

export enum TreeItemCollapsibleState {
  None = 0,
  Collapsed = 1,
  Expanded = 2,
}

export enum StatusBarAlignment {
  Left = 1,
  Right = 2,
}

export enum ExtensionKind {
  UI = 1,
  Workspace = 2,
}

export enum UIKind {
  Desktop = 1,
  Web = 2,
}

export class TreeItem {
  label: string | undefined;
  collapsibleState: TreeItemCollapsibleState | undefined;
  contextValue: string | undefined;
  iconPath: unknown;
  description: string | undefined;
  tooltip: unknown;
  command: unknown;
  id: string | undefined;
  resourceUri: unknown;

  constructor(
    label: string | { label: string },
    collapsibleState?: TreeItemCollapsibleState,
  ) {
    this.label = typeof label === 'string' ? label : label.label;
    this.collapsibleState = collapsibleState;
  }
}

export class ThemeIcon {
  constructor(public id: string) {}
}

export class Uri {
  static file(path: string): Uri {
    return new Uri(path);
  }
  static parse(path: string): Uri {
    return new Uri(path);
  }
  constructor(public fsPath: string) {}
}

export class MarkdownString {
  constructor(
    public value: string,
    public supportThemeIcons?: boolean,
  ) {}
  isTrusted = false;
}

export class EventEmitter<T> {
  private _listeners: Array<(e: T) => void> = [];

  event = (listener: (e: T) => void): { dispose: () => void } => {
    this._listeners.push(listener);
    return {
      dispose: () => {
        this._listeners = this._listeners.filter(l => l !== listener);
      },
    };
  };

  fire(data: T): void {
    for (const listener of [...this._listeners]) {
      listener(data);
    }
  }

  dispose(): void {
    this._listeners = [];
  }
}

export class CancellationToken {
  isCancellationRequested = false;
}

export const window = {
  createOutputChannel: jest.fn(() => ({
    append: jest.fn(),
    appendLine: jest.fn(),
    dispose: jest.fn(),
  })),
  createTreeView: jest.fn(() => ({
    message: undefined,
    dispose: jest.fn(),
  })),
  createStatusBarItem: jest.fn(() => ({
    command: undefined,
    tooltip: undefined,
    text: '',
    show: jest.fn(),
    dispose: jest.fn(),
  })),
  showErrorMessage: jest.fn(),
  showInformationMessage: jest.fn(),
  showWarningMessage: jest.fn(),
  showQuickPick: jest.fn(),
  showInputBox: jest.fn(),
  setStatusBarMessage: jest.fn(),
};

export const env = {
  clipboard: {
    writeText: jest.fn(() => Promise.resolve()),
  },
  openExternal: jest.fn(() => Promise.resolve(true)),
};

export const workspace = {
  getConfiguration: jest.fn(() => ({
    get: jest.fn(),
  })),
  onDidChangeWorkspaceFolders: jest.fn(() => ({ dispose: () => {} })),
  onDidChangeConfiguration: jest.fn(() => ({ dispose: () => {} })),
  workspaceFolders: [],
};

export const commands = {
  registerCommand: jest.fn(() => ({ dispose: () => {} })),
  executeCommand: jest.fn(),
};
