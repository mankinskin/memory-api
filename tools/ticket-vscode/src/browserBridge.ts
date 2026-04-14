// Browser Bridge: HTTP control server + CDP-based Playwright automation.
//
// Exposes a local HTTP API that MCP tools or CLI scripts can call to:
//   - Open/navigate VS Code's Simple Browser to a URL
//   - Interact with the page via Playwright (click, fill, screenshot, snapshot, evaluate)
//
// Requires VS Code to be launched with --remote-debugging-port=<port>.

import * as vscode from 'vscode';
import * as http from 'node:http';
import type { AddressInfo } from 'node:net';

// Playwright types — loaded lazily via dynamic require since the package is
// optional.  We only use structural shapes here so we don't need the real
// module at compile time.

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

/** Ports to probe when auto-discovering CDP. */
const CDP_PROBE_PORTS = [9222, 9223, 9229, 9230];

export interface BridgeConfig {
  /** Port for the HTTP control server. 0 = auto-assign. */
  controlPort: number;
  /** CDP debugging port of the VS Code / Electron process. 0 = auto-discover. */
  cdpPort: number;
  /** Try to connect to CDP automatically on startup. */
  autoConnectCdp: boolean;
}

interface BridgeState {
  /** The URL currently shown in Simple Browser (best-effort tracking). */
  currentUrl: string | null;
  /** Whether a CDP connection to the Electron host is established. */
  cdpConnected: boolean;
  /** The control server port actually in use. */
  controlPort: number;
}

/**
 * The BrowserBridge manages:
 * 1. A local HTTP control server for external callers (MCP tools, CLI).
 * 2. Simple Browser navigation via VS Code commands.
 * 3. Optional Playwright-over-CDP connection for page automation.
 */
export class BrowserBridge implements vscode.Disposable {
  private _server: http.Server | null = null;
  private _browser: PwBrowser | null = null;
  private _page: PwPage | null = null;
  private _playwright: PwModule | null = null;
  private _currentUrl: string | null = null;
  private _config: BridgeConfig;
  private _outputChannel: vscode.OutputChannel;

  constructor(config: BridgeConfig) {
    this._config = config;
    this._outputChannel = vscode.window.createOutputChannel('Browser Bridge');
  }

  get state(): BridgeState {
    return {
      currentUrl: this._currentUrl,
      cdpConnected: this._browser !== null,
      controlPort: (this._server?.address() as AddressInfo | null)?.port ?? 0,
    };
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  async start(): Promise<number> {
    if (this._server) { return this.state.controlPort; }

    const server = http.createServer((req, res) => {
      this._handleRequest(req, res);
    });

    const port = await new Promise<number>((resolve, reject) => {
      server.listen(this._config.controlPort, '127.0.0.1', () => {
        this._server = server;
        const p = (server.address() as AddressInfo).port;
        this._outputChannel.appendLine(`Browser Bridge control server listening on http://127.0.0.1:${p}`);
        vscode.window.setStatusBarMessage(`$(plug) Browser Bridge running on port ${p}`, 5000);
        resolve(p);
      });
      server.on('error', reject);
    });

    // Auto-connect CDP if configured.
    if (this._config.autoConnectCdp) {
      // Run in background — don't block startup.
      this._autoConnectCdp();
    }

    return port;
  }

  /**
   * Silently attempt CDP connection. If cdpPort is 0, probe common ports.
   * Never shows UI warnings — just logs to the output channel.
   */
  private async _autoConnectCdp(): Promise<void> {
    // Small delay: give VS Code's renderer processes time to fully start.
    await new Promise(r => setTimeout(r, 2000));

    const portsToTry = this._config.cdpPort > 0
      ? [this._config.cdpPort]
      : CDP_PROBE_PORTS;

    for (const port of portsToTry) {
      const available = await this._probeCdpPort(port);
      if (available) {
        this._outputChannel.appendLine(`CDP auto-discovered on port ${port}`);
        const ok = await this.connectCdp({ port, silent: true });
        if (ok) { return; }
      }
    }

    this._outputChannel.appendLine(
      'CDP auto-connect: no reachable port found. ' +
      'Launch VS Code with --remote-debugging-port=9222 for CDP automation.'
    );
  }

  /**
   * Check if a CDP endpoint is reachable by fetching /json/version.
   */
  private _probeCdpPort(port: number): Promise<boolean> {
    return new Promise(resolve => {
      const req = http.get(`http://127.0.0.1:${port}/json/version`, { timeout: 1500 }, res => {
        // Drain the response.
        res.resume();
        resolve(res.statusCode === 200);
      });
      req.on('error', () => resolve(false));
      req.on('timeout', () => { req.destroy(); resolve(false); });
    });
  }

  async dispose(): Promise<void> {
    await this._disconnectCdp();
    if (this._server) {
      await new Promise<void>((resolve) => {
        this._server!.close(() => resolve());
      });
      this._server = null;
    }
    this._outputChannel.dispose();
  }

  // ── Simple Browser control ─────────────────────────────────────────────────

  async navigate(url: string): Promise<void> {
    this._currentUrl = url;
    await vscode.commands.executeCommand('simpleBrowser.show', url);
    this._outputChannel.appendLine(`Navigated Simple Browser to ${url}`);

    // If CDP is connected, also try to find and target the page.
    if (this._browser) {
      await this._findTargetPage(url);
    }
  }

  // ── CDP / Playwright connection ────────────────────────────────────────────

  /**
   * Connect to CDP.
   * @param opts.port  Override the configured CDP port.
   * @param opts.silent  If true, don't show UI warnings on failure (used for auto-connect).
   */
  async connectCdp(opts?: { port?: number; silent?: boolean }): Promise<boolean> {
    if (this._browser) { return true; }

    const port = opts?.port ?? this._config.cdpPort;
    const silent = opts?.silent ?? false;

    let pw: PwModule;
    try {
      // Dynamic require — playwright must be installed in the extension's
      // node_modules or globally available.
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
      this._playwright = pw;
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

  private async _disconnectCdp(): Promise<void> {
    if (this._browser) {
      try { await this._browser.close(); } catch { /* ignore */ }
      this._browser = null;
      this._page = null;
    }
  }

  /** Scan CDP contexts to find the Simple Browser page showing `url`. */
  private async _findTargetPage(url: string): Promise<PwPage | null> {
    if (!this._browser) { return null; }

    for (const context of this._browser.contexts()) {
      for (const page of context.pages()) {
        const pageUrl: string = page.url();
        // Simple Browser wraps URLs — check both exact match and contains.
        if (pageUrl === url || pageUrl.includes(url)) {
          this._page = page;
          this._outputChannel.appendLine(`Found CDP target for ${url}`);
          return page;
        }
      }
    }

    // The page may be in a frame inside a webview wrapper.
    for (const context of this._browser.contexts()) {
      for (const page of context.pages()) {
        for (const frame of page.frames()) {
          const frameUrl: string = frame.url();
          if (frameUrl === url || frameUrl.includes(url)) {
            this._page = page;
            this._outputChannel.appendLine(`Found CDP target in frame for ${url}`);
            return page;
          }
        }
      }
    }

    this._outputChannel.appendLine(`No CDP target found for ${url}`);
    return null;
  }

  // ── Page automation (requires CDP) ─────────────────────────────────────────

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
    // Accessibility tree snapshot — Playwright's built-in method.
    try {
      const snap = await this._page.accessibility.snapshot();
      return JSON.stringify(snap, null, 2);
    } catch {
      // Fallback: return page content.
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

  // ── HTTP control server handler ────────────────────────────────────────────

  private _handleRequest(req: http.IncomingMessage, res: http.ServerResponse): void {
    const url = new URL(req.url ?? '/', `http://${req.headers.host ?? 'localhost'}`);
    const path = url.pathname;
    const method = req.method ?? 'GET';

    // CORS headers for local dev tools.
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

    if (method === 'OPTIONS') {
      res.writeHead(204);
      res.end();
      return;
    }

    // Route dispatch.
    if (method === 'GET' && path === '/status') {
      this._handleStatus(res);
    } else if (method === 'POST' && path === '/navigate') {
      this._readBody(req).then(body => this._handleNavigate(body, res)).catch(e => this._error(res, e));
    } else if (method === 'POST' && path === '/connect-cdp') {
      this._handleConnectCdp(res);
    } else if (method === 'POST' && path === '/click') {
      this._readBody(req).then(body => this._handleClick(body, res)).catch(e => this._error(res, e));
    } else if (method === 'POST' && path === '/fill') {
      this._readBody(req).then(body => this._handleFill(body, res)).catch(e => this._error(res, e));
    } else if (method === 'POST' && path === '/screenshot') {
      this._handleScreenshot(res);
    } else if (method === 'POST' && path === '/snapshot') {
      this._handleSnapshot(res);
    } else if (method === 'POST' && path === '/evaluate') {
      this._readBody(req).then(body => this._handleEvaluate(body, res)).catch(e => this._error(res, e));
    } else if (method === 'GET' && path === '/pages') {
      this._handleListPages(res);
    } else if (method === 'POST' && path === '/close') {
      this._handleClose(res);
    } else {
      this._json(res, 404, { error: 'Not found', endpoints: [
        'GET  /status', 'POST /navigate', 'POST /connect-cdp',
        'POST /click', 'POST /fill', 'POST /screenshot',
        'POST /snapshot', 'POST /evaluate', 'GET  /pages', 'POST /close',
      ]});
    }
  }

  // ── Route handlers ─────────────────────────────────────────────────────────

  private _handleStatus(res: http.ServerResponse): void {
    this._json(res, 200, this.state);
  }

  private async _handleNavigate(body: Record<string, unknown>, res: http.ServerResponse): Promise<void> {
    const url = body['url'];
    if (typeof url !== 'string' || !url) {
      this._json(res, 400, { error: 'Missing "url" field' });
      return;
    }
    await this.navigate(url);
    this._json(res, 200, { ok: true, url });
  }

  private async _handleConnectCdp(res: http.ServerResponse): Promise<void> {
    const connected = await this.connectCdp();
    this._json(res, connected ? 200 : 502, { connected });
  }

  private async _handleClick(body: Record<string, unknown>, res: http.ServerResponse): Promise<void> {
    const selector = body['selector'];
    if (typeof selector !== 'string') {
      this._json(res, 400, { error: 'Missing "selector" field' });
      return;
    }
    const ok = await this.click(selector);
    this._json(res, ok ? 200 : 503, { ok, ...(ok ? {} : { error: 'No page connected via CDP' }) });
  }

  private async _handleFill(body: Record<string, unknown>, res: http.ServerResponse): Promise<void> {
    const selector = body['selector'];
    const value = body['value'];
    if (typeof selector !== 'string' || typeof value !== 'string') {
      this._json(res, 400, { error: 'Missing "selector" and/or "value" fields' });
      return;
    }
    const ok = await this.fill(selector, value);
    this._json(res, ok ? 200 : 503, { ok, ...(ok ? {} : { error: 'No page connected via CDP' }) });
  }

  private async _handleScreenshot(res: http.ServerResponse): Promise<void> {
    const buf = await this.screenshot();
    if (!buf) {
      this._json(res, 503, { error: 'No page connected via CDP' });
      return;
    }
    res.writeHead(200, { 'Content-Type': 'image/png' });
    res.end(buf);
  }

  private async _handleSnapshot(res: http.ServerResponse): Promise<void> {
    const snap = await this.snapshot();
    if (snap === null) {
      this._json(res, 503, { error: 'No page connected via CDP' });
      return;
    }
    this._json(res, 200, { snapshot: JSON.parse(snap) });
  }

  private async _handleEvaluate(body: Record<string, unknown>, res: http.ServerResponse): Promise<void> {
    const expression = body['expression'];
    if (typeof expression !== 'string') {
      this._json(res, 400, { error: 'Missing "expression" field' });
      return;
    }
    try {
      const result = await this.evaluate(expression);
      this._json(res, 200, { result });
    } catch (err) {
      this._json(res, 500, { error: err instanceof Error ? err.message : String(err) });
    }
  }

  private async _handleListPages(res: http.ServerResponse): Promise<void> {
    const pages = await this.listPages();
    this._json(res, 200, { pages });
  }

  private async _handleClose(res: http.ServerResponse): Promise<void> {
    this._currentUrl = null;
    // There's no VS Code command to close Simple Browser, but we can disconnect CDP.
    await this._disconnectCdp();
    this._json(res, 200, { ok: true });
  }

  // ── Helpers ────────────────────────────────────────────────────────────────

  private _readBody(req: http.IncomingMessage): Promise<Record<string, unknown>> {
    return new Promise((resolve, reject) => {
      const chunks: Buffer[] = [];
      let size = 0;
      const maxSize = 1024 * 1024; // 1 MB limit

      req.on('data', (chunk: Buffer) => {
        size += chunk.length;
        if (size > maxSize) {
          req.destroy();
          reject(new Error('Request body too large'));
          return;
        }
        chunks.push(chunk);
      });
      req.on('end', () => {
        try {
          const raw = Buffer.concat(chunks).toString('utf-8');
          resolve(raw ? JSON.parse(raw) : {});
        } catch {
          reject(new Error('Invalid JSON'));
        }
      });
      req.on('error', reject);
    });
  }

  private _json(res: http.ServerResponse, status: number, data: unknown): void {
    const body = JSON.stringify(data);
    res.writeHead(status, { 'Content-Type': 'application/json' });
    res.end(body);
  }

  private _error(res: http.ServerResponse, err: unknown): void {
    const msg = err instanceof Error ? err.message : String(err);
    this._outputChannel.appendLine(`Error: ${msg}`);
    this._json(res, 500, { error: msg });
  }
}
