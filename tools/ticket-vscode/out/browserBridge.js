"use strict";
// Browser Bridge: HTTP control server + CDP-based Playwright automation.
//
// Exposes a local HTTP API that MCP tools or CLI scripts can call to:
//   - Open/navigate VS Code's Simple Browser to a URL
//   - Interact with the page via Playwright (click, fill, screenshot, snapshot, evaluate)
//
// Requires VS Code to be launched with --remote-debugging-port=<port>.
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.BrowserBridge = void 0;
const vscode = __importStar(require("vscode"));
const http = __importStar(require("node:http"));
/** Ports to probe when auto-discovering CDP. */
const CDP_PROBE_PORTS = [9222, 9223, 9229, 9230];
/**
 * The BrowserBridge manages:
 * 1. A local HTTP control server for external callers (MCP tools, CLI).
 * 2. Simple Browser navigation via VS Code commands.
 * 3. Optional Playwright-over-CDP connection for page automation.
 */
class BrowserBridge {
    constructor(config) {
        this._server = null;
        this._browser = null;
        this._page = null;
        this._playwright = null;
        this._currentUrl = null;
        this._config = config;
        this._outputChannel = vscode.window.createOutputChannel('Browser Bridge');
    }
    get state() {
        return {
            currentUrl: this._currentUrl,
            cdpConnected: this._browser !== null,
            controlPort: this._server?.address()?.port ?? 0,
        };
    }
    // ── Lifecycle ──────────────────────────────────────────────────────────────
    async start() {
        if (this._server) {
            return this.state.controlPort;
        }
        const server = http.createServer((req, res) => {
            this._handleRequest(req, res);
        });
        const port = await new Promise((resolve, reject) => {
            server.listen(this._config.controlPort, '127.0.0.1', () => {
                this._server = server;
                const p = server.address().port;
                this._outputChannel.appendLine(`Browser Bridge control server listening on http://127.0.0.1:${p}`);
                void vscode.window.showInformationMessage(`Browser Bridge running on port ${p}`);
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
    async _autoConnectCdp() {
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
                if (ok) {
                    return;
                }
            }
        }
        this._outputChannel.appendLine('CDP auto-connect: no reachable port found. ' +
            'Launch VS Code with --remote-debugging-port=9222 for CDP automation.');
    }
    /**
     * Check if a CDP endpoint is reachable by fetching /json/version.
     */
    _probeCdpPort(port) {
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
    async dispose() {
        await this._disconnectCdp();
        if (this._server) {
            await new Promise((resolve) => {
                this._server.close(() => resolve());
            });
            this._server = null;
        }
        this._outputChannel.dispose();
    }
    // ── Simple Browser control ─────────────────────────────────────────────────
    async navigate(url) {
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
    async connectCdp(opts) {
        if (this._browser) {
            return true;
        }
        const port = opts?.port ?? this._config.cdpPort;
        const silent = opts?.silent ?? false;
        let pw;
        try {
            // Dynamic require — playwright must be installed in the extension's
            // node_modules or globally available.
            // eslint-disable-next-line @typescript-eslint/no-require-imports
            pw = require('playwright');
        }
        catch {
            this._outputChannel.appendLine('Playwright not found. Install it with: npm i playwright (in the extension folder)');
            if (!silent) {
                void vscode.window.showWarningMessage('Browser Bridge: playwright package not found. CDP automation disabled.');
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
        }
        catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            this._outputChannel.appendLine(`CDP connect failed: ${msg}`);
            if (!silent) {
                this._outputChannel.appendLine('Make sure VS Code was launched with: code --remote-debugging-port=' + port);
                void vscode.window.showWarningMessage(`Browser Bridge: Could not connect to CDP on port ${port}. ` +
                    'Launch VS Code with --remote-debugging-port=' + port);
            }
            return false;
        }
    }
    async _disconnectCdp() {
        if (this._browser) {
            try {
                await this._browser.close();
            }
            catch { /* ignore */ }
            this._browser = null;
            this._page = null;
        }
    }
    /** Scan CDP contexts to find the Simple Browser page showing `url`. */
    async _findTargetPage(url) {
        if (!this._browser) {
            return null;
        }
        for (const context of this._browser.contexts()) {
            for (const page of context.pages()) {
                const pageUrl = page.url();
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
                    const frameUrl = frame.url();
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
    async click(selector) {
        if (!this._page) {
            return false;
        }
        await this._page.click(selector);
        return true;
    }
    async fill(selector, value) {
        if (!this._page) {
            return false;
        }
        await this._page.fill(selector, value);
        return true;
    }
    async screenshot() {
        if (!this._page) {
            return null;
        }
        return this._page.screenshot();
    }
    async snapshot() {
        if (!this._page) {
            return null;
        }
        // Accessibility tree snapshot — Playwright's built-in method.
        try {
            const snap = await this._page.accessibility.snapshot();
            return JSON.stringify(snap, null, 2);
        }
        catch {
            // Fallback: return page content.
            return this._page.content();
        }
    }
    async evaluate(expression) {
        if (!this._page) {
            return { error: 'No page connected' };
        }
        return this._page.evaluate(expression);
    }
    async listPages() {
        if (!this._browser) {
            return [];
        }
        const pages = [];
        for (const context of this._browser.contexts()) {
            for (const page of context.pages()) {
                pages.push({ url: page.url(), title: await page.title() });
            }
        }
        return pages;
    }
    // ── HTTP control server handler ────────────────────────────────────────────
    _handleRequest(req, res) {
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
        }
        else if (method === 'POST' && path === '/navigate') {
            this._readBody(req).then(body => this._handleNavigate(body, res)).catch(e => this._error(res, e));
        }
        else if (method === 'POST' && path === '/connect-cdp') {
            this._handleConnectCdp(res);
        }
        else if (method === 'POST' && path === '/click') {
            this._readBody(req).then(body => this._handleClick(body, res)).catch(e => this._error(res, e));
        }
        else if (method === 'POST' && path === '/fill') {
            this._readBody(req).then(body => this._handleFill(body, res)).catch(e => this._error(res, e));
        }
        else if (method === 'POST' && path === '/screenshot') {
            this._handleScreenshot(res);
        }
        else if (method === 'POST' && path === '/snapshot') {
            this._handleSnapshot(res);
        }
        else if (method === 'POST' && path === '/evaluate') {
            this._readBody(req).then(body => this._handleEvaluate(body, res)).catch(e => this._error(res, e));
        }
        else if (method === 'GET' && path === '/pages') {
            this._handleListPages(res);
        }
        else if (method === 'POST' && path === '/close') {
            this._handleClose(res);
        }
        else {
            this._json(res, 404, { error: 'Not found', endpoints: [
                    'GET  /status', 'POST /navigate', 'POST /connect-cdp',
                    'POST /click', 'POST /fill', 'POST /screenshot',
                    'POST /snapshot', 'POST /evaluate', 'GET  /pages', 'POST /close',
                ] });
        }
    }
    // ── Route handlers ─────────────────────────────────────────────────────────
    _handleStatus(res) {
        this._json(res, 200, this.state);
    }
    async _handleNavigate(body, res) {
        const url = body['url'];
        if (typeof url !== 'string' || !url) {
            this._json(res, 400, { error: 'Missing "url" field' });
            return;
        }
        await this.navigate(url);
        this._json(res, 200, { ok: true, url });
    }
    async _handleConnectCdp(res) {
        const connected = await this.connectCdp();
        this._json(res, connected ? 200 : 502, { connected });
    }
    async _handleClick(body, res) {
        const selector = body['selector'];
        if (typeof selector !== 'string') {
            this._json(res, 400, { error: 'Missing "selector" field' });
            return;
        }
        const ok = await this.click(selector);
        this._json(res, ok ? 200 : 503, { ok, ...(ok ? {} : { error: 'No page connected via CDP' }) });
    }
    async _handleFill(body, res) {
        const selector = body['selector'];
        const value = body['value'];
        if (typeof selector !== 'string' || typeof value !== 'string') {
            this._json(res, 400, { error: 'Missing "selector" and/or "value" fields' });
            return;
        }
        const ok = await this.fill(selector, value);
        this._json(res, ok ? 200 : 503, { ok, ...(ok ? {} : { error: 'No page connected via CDP' }) });
    }
    async _handleScreenshot(res) {
        const buf = await this.screenshot();
        if (!buf) {
            this._json(res, 503, { error: 'No page connected via CDP' });
            return;
        }
        res.writeHead(200, { 'Content-Type': 'image/png' });
        res.end(buf);
    }
    async _handleSnapshot(res) {
        const snap = await this.snapshot();
        if (snap === null) {
            this._json(res, 503, { error: 'No page connected via CDP' });
            return;
        }
        this._json(res, 200, { snapshot: JSON.parse(snap) });
    }
    async _handleEvaluate(body, res) {
        const expression = body['expression'];
        if (typeof expression !== 'string') {
            this._json(res, 400, { error: 'Missing "expression" field' });
            return;
        }
        try {
            const result = await this.evaluate(expression);
            this._json(res, 200, { result });
        }
        catch (err) {
            this._json(res, 500, { error: err instanceof Error ? err.message : String(err) });
        }
    }
    async _handleListPages(res) {
        const pages = await this.listPages();
        this._json(res, 200, { pages });
    }
    async _handleClose(res) {
        this._currentUrl = null;
        // There's no VS Code command to close Simple Browser, but we can disconnect CDP.
        await this._disconnectCdp();
        this._json(res, 200, { ok: true });
    }
    // ── Helpers ────────────────────────────────────────────────────────────────
    _readBody(req) {
        return new Promise((resolve, reject) => {
            const chunks = [];
            let size = 0;
            const maxSize = 1024 * 1024; // 1 MB limit
            req.on('data', (chunk) => {
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
                }
                catch {
                    reject(new Error('Invalid JSON'));
                }
            });
            req.on('error', reject);
        });
    }
    _json(res, status, data) {
        const body = JSON.stringify(data);
        res.writeHead(status, { 'Content-Type': 'application/json' });
        res.end(body);
    }
    _error(res, err) {
        const msg = err instanceof Error ? err.message : String(err);
        this._outputChannel.appendLine(`Error: ${msg}`);
        this._json(res, 500, { error: msg });
    }
}
exports.BrowserBridge = BrowserBridge;
//# sourceMappingURL=browserBridge.js.map