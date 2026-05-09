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
import { BrowserBridgeCdpClient } from './browserBridgeCdp';

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
  private readonly _cdp: BrowserBridgeCdpClient;
  private _currentUrl: string | null = null;
  private _config: BridgeConfig;
  private _outputChannel: vscode.OutputChannel;

  constructor(config: BridgeConfig) {
    this._config = config;
    this._outputChannel = vscode.window.createOutputChannel('Browser Bridge');
    this._cdp = new BrowserBridgeCdpClient(this._outputChannel);
  }

  get state(): BridgeState {
    return {
      currentUrl: this._currentUrl,
      cdpConnected: this._cdp.connected,
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
    await this._cdp.disconnect();
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

    if (this._cdp.connected) {
      await this._cdp.findTargetPage(url);
    }
  }

  // ── CDP / Playwright connection ────────────────────────────────────────────

  /**
   * Connect to CDP.
   * @param opts.port  Override the configured CDP port.
   * @param opts.silent  If true, don't show UI warnings on failure (used for auto-connect).
   */
  async connectCdp(opts?: { port?: number; silent?: boolean }): Promise<boolean> {
    const port = opts?.port ?? this._config.cdpPort;
    const silent = opts?.silent ?? false;
    return this._cdp.connect(port, silent);
  }

  // ── Page automation (requires CDP) ─────────────────────────────────────────

  async click(selector: string): Promise<boolean> {
    return this._cdp.click(selector);
  }

  async fill(selector: string, value: string): Promise<boolean> {
    return this._cdp.fill(selector, value);
  }

  async screenshot(): Promise<Buffer | null> {
    return this._cdp.screenshot();
  }

  async snapshot(): Promise<string | null> {
    return this._cdp.snapshot();
  }

  async evaluate(expression: string): Promise<unknown> {
    return this._cdp.evaluate(expression);
  }

  async listPages(): Promise<Array<{ url: string; title: string }>> {
    return this._cdp.listPages();
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
    await this._cdp.disconnect();
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
