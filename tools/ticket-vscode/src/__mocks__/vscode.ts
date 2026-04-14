// Minimal vscode mock for unit testing ticketProvider logic.

export enum TreeItemCollapsibleState {
  None = 0,
  Collapsed = 1,
  Expanded = 2,
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
  showErrorMessage: jest.fn(),
  showInformationMessage: jest.fn(),
};

export const workspace = {
  getConfiguration: jest.fn(() => ({
    get: jest.fn(),
  })),
};

export const commands = {
  registerCommand: jest.fn(() => ({ dispose: () => {} })),
  executeCommand: jest.fn(),
};
