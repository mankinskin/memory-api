import * as vscode from 'vscode';

/** Minimal structural type matching Playwright's Browser interface. */
interface PwBrowser {
  contexts(): PwContext[];
  close(): Promise<void>;
}

interface PwContext {
  pages(): PwPage[];
}

interface PwPage {
  url(): string;
  title(): Promise<string>;
  frames(): PwFrame[];
  click(selector: string): Promise<void>;
  fill(selector: string, value: string): Promise<void>;
  screenshot(): Promise<Buffer>;
  content(): Promise<string>;
  evaluate(expression: string): Promise<unknown>;
  accessibility: { snapshot(): Promise<unknown> };
}

interface PwFrame {
  url(): string;
}

/** Minimal structural type for the playwright module's top-level export. */
interface PwModule {
  chromium: {
    connectOverCDP(endpoint: string): Promise<PwBrowser>;
  };
}

export class BrowserBridgeCdpClient {
  private _browser: PwBrowser | null = null;
  private _page: PwPage | null = null;

  constructor(private readonly _outputChannel: vscode.OutputChannel) {}

  get connected(): boolean {
    return this._browser !== null;
  }

  async connect(port: number, silent: boolean): Promise<boolean> {
    if (this._browser) { return true; }

    let pw: PwModule;
    try {
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      pw = require('playwright') as PwModule;
    } catch {
      this._outputChannel.appendLine(
        'Playwright not found. Install it with: npm i playwright (in the extension folder)'
      );
      if (!silent) {
        void vscode.window.showWarningMessage(
          'Browser Bridge: playwright package not found. CDP automation disabled.'
        );
      }
      return false;
    }

    try {
      const endpoint = `http://127.0.0.1:${port}`;
      this._outputChannel.appendLine(`Connecting to CDP at ${endpoint}…`);
      this._browser = await pw.chromium.connectOverCDP(endpoint);
      this._outputChannel.appendLine('CDP connection established.');
      return true;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      this._outputChannel.appendLine(`CDP connect failed: ${msg}`);
      if (!silent) {
        this._outputChannel.appendLine(
          'Make sure VS Code was launched with: code --remote-debugging-port=' + port
        );
        void vscode.window.showWarningMessage(
          `Browser Bridge: Could not connect to CDP on port ${port}. ` +
          'Launch VS Code with --remote-debugging-port=' + port
        );
      }
      return false;
    }
  }

  async disconnect(): Promise<void> {
    if (this._browser) {
      try { await this._browser.close(); } catch { /* ignore */ }
      this._browser = null;
      this._page = null;
    }
  }

  async findTargetPage(url: string): Promise<boolean> {
    if (!this._browser) { return false; }

    for (const context of this._browser.contexts()) {
      for (const page of context.pages()) {
        const pageUrl: string = page.url();
        if (pageUrl === url || pageUrl.includes(url)) {
          this._page = page;
          this._outputChannel.appendLine(`Found CDP target for ${url}`);
          return true;
        }
      }
    }

    for (const context of this._browser.contexts()) {
      for (const page of context.pages()) {
        for (const frame of page.frames()) {
          const frameUrl: string = frame.url();
          if (frameUrl === url || frameUrl.includes(url)) {
            this._page = page;
            this._outputChannel.appendLine(`Found CDP target in frame for ${url}`);
            return true;
          }
        }
      }
    }

    this._outputChannel.appendLine(`No CDP target found for ${url}`);
    return false;
  }

  async click(selector: string): Promise<boolean> {
    if (!this._page) { return false; }
    await this._page.click(selector);
    return true;
  }

  async fill(selector: string, value: string): Promise<boolean> {
    if (!this._page) { return false; }
    await this._page.fill(selector, value);
    return true;
  }

  async screenshot(): Promise<Buffer | null> {
    if (!this._page) { return null; }
    return this._page.screenshot() as Promise<Buffer>;
  }

  async snapshot(): Promise<string | null> {
    if (!this._page) { return null; }
    try {
      const snapshot = await this._page.accessibility.snapshot();
      return JSON.stringify(snapshot, null, 2);
    } catch {
      return this._page.content() as Promise<string>;
    }
  }

  async evaluate(expression: string): Promise<unknown> {
    if (!this._page) { return { error: 'No page connected' }; }
    return this._page.evaluate(expression);
  }

  async listPages(): Promise<Array<{ url: string; title: string }>> {
    if (!this._browser) { return []; }
    const pages: Array<{ url: string; title: string }> = [];
    for (const context of this._browser.contexts()) {
      for (const page of context.pages()) {
        pages.push({ url: page.url(), title: await page.title() });
      }
    }
    return pages;
  }
}