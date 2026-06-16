// coreLoader.ts
//
// Shared WASM core loader for both the Node (desktop/remote) and browser
// extension hosts.
//
// Design contract (frozen in spec ticket-vscode/rust-wasm-port a592900c):
// - No vscode or Node I/O in this file. All host I/O is injected by the caller.
// - Both extension.ts and extension.browser.ts call initWasmCore() passing only
//   the raw WASM bytes they read via vscode.workspace.fs.
// - CoreApi is the stable abstraction consumed by TicketTreeProvider; it never
//   exposes wasm-bindgen internals to callers.

import type { TicketSummary as ApiTicketSummary, EdgeRecord as ApiEdgeRecord } from './api';
import type { HostKind as TsHostKind } from './hostCapabilities';

// ── Public types ──────────────────────────────────────────────────────────────

/** Result of WasmDependencyMaps::build — plain JS Maps for O(1) lookups. */
export interface DependencyMapsResult {
  /** ticket id → ids of tickets it depends_on */
  depsOf: Map<string, string[]>;
  /** ticket id → ids of its parents (reverse of depsOf) */
  parentOf: Map<string, string[]>;
}

/** A state-grouped bucket, mirroring WasmStateGroup. */
export interface StateGroupData {
  state: string;
  total: number;
  rootIds: string[];
}

/** Stable interface that TicketTreeProvider depends on. */
export interface CoreApi {
  core_version(): string;

  // Filtering
  ticket_matches(ticket: ApiTicketSummary, stateFilter: string, query: string): boolean;

  // Dependency maps (built from edges; returns plain Maps for fast rendering)
  build_dependency_maps(tickets: ApiTicketSummary[], edges: ApiEdgeRecord[]): DependencyMapsResult;

  // State grouping
  build_state_groups(
    tickets: ApiTicketSummary[],
    edges: ApiEdgeRecord[],
    stateOrder: string[],
    query: string,
  ): StateGroupData[];

  // Host-kind gates
  supports_server_control(hostKind: TsHostKind): boolean;
  supports_browser_bridge(hostKind: TsHostKind): boolean;
  supports_file_browsing(hostKind: TsHostKind): boolean;

  // URL / label derivation
  ticket_viewer_url(baseUrl: string, workspace: string, ticketId: string): string;
  ticket_display_label(id: string, title: string | null): string;
}

// ── HostKind mapping ──────────────────────────────────────────────────────────

// Numeric values correspond to the WASM HostKind enum (DesktopNode=0,
// RemoteWorkspace=1, BrowserWeb=2, Virtual=3).
const HOST_KIND_MAP: Record<TsHostKind, number> = {
  'desktop-node': 0,
  'remote-workspace': 1,
  'browser-web': 2,
  'virtual': 3,
};

// ── WASM initialisation ───────────────────────────────────────────────────────

// These are lazily-resolved module-level caches so we only initialise once.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let _bgModule: Record<string, unknown> | null = null;
let _initialised = false;

/**
 * Initialise the WASM core from raw bytes.
 *
 * The `wasmBytes` are obtained by the caller via `vscode.workspace.fs.readFile`
 * so this function itself has no I/O and works identically in both hosts.
 *
 * Returns a `CoreApi` instance. Throws on fatal errors (e.g. wrong WASM binary).
 */
export async function initWasmCore(wasmBytes: Uint8Array): Promise<CoreApi> {
  if (!_initialised) {
    // Dynamically import the wasm-bindgen JS glue.
    // esbuild inlines ticket_vscode_core_bg.js when bundling both hosts; the
    // static .wasm import inside ticket_vscode_core.js is excluded via the
    // '.wasm': 'empty' loader so we initialise the WASM manually here.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const bg = await import('../pkg/ticket_vscode_core_bg.js') as Record<string, unknown>;
    _bgModule = bg;

    // Build the import object by inspecting the WASM module's import section.
    // This is robust against wasm-bindgen version changes since we don't hard-
    // code the import names.
    const wasmModule = await WebAssembly.compile(wasmBytes.buffer as ArrayBuffer);
    const importDescriptors = WebAssembly.Module.imports(wasmModule);
    const importObj: Record<string, Record<string, unknown>> = {};
    for (const { module, name } of importDescriptors) {
      if (!importObj[module]) { importObj[module] = {}; }
      const fn = bg[name];
      if (fn !== undefined) {
        importObj[module][name] = fn;
      }
    }

    const instance = await WebAssembly.instantiate(
      wasmModule,
      importObj as unknown as WebAssembly.Imports,
    );
    const setWasm = bg['__wbg_set_wasm'] as (exports: WebAssembly.Exports) => void;
    setWasm(instance.exports);

    const exports = instance.exports as Record<string, unknown>;
    if (typeof exports['__wbindgen_start'] === 'function') {
      (exports['__wbindgen_start'] as () => void)();
    }

    _initialised = true;
  }

  return createCoreApi(_bgModule!);
}

// ── WasmCoreApi ───────────────────────────────────────────────────────────────

function createCoreApi(bg: Record<string, unknown>): CoreApi {
  // Retrieve the exported functions from the wasm-bindgen glue module.
  // These are safe to call after __wbg_set_wasm() has been called above.
  const core_version = bg['core_version'] as () => string;
  const ticket_matches_fn = bg['ticket_matches'] as (
    t: object, sf: string, q: string,
  ) => boolean;
  const supports_server_control_fn = bg['supports_server_control'] as (h: number) => boolean;
  const supports_browser_bridge_fn = bg['supports_browser_bridge'] as (h: number) => boolean;
  const supports_file_browsing_fn = bg['supports_file_browsing'] as (h: number) => boolean;
  const ticket_viewer_url_fn = bg['ticket_viewer_url'] as (
    b: string, w: string, id: string,
  ) => string;
  const ticket_display_label_fn = bg['ticket_display_label'] as (
    id: string, title: string,
  ) => string;
  const TicketSummaryClass = bg['TicketSummary'] as new (
    id: string, ticket_type: string, title: string, state: string,
  ) => object;
  const WasmDependencyMapsClass = bg['WasmDependencyMaps'] as {
    build(ids: string[], froms: string[], tos: string[], kinds: string[]): {
      depsOf(id: string): string[];
      parentOf(id: string): string[];
    };
  };
  const buildStateGroups_fn = bg['buildStateGroups'] as (
    ids: string[], titles: string[], states: string[],
    froms: string[], tos: string[], kinds: string[],
    stateOrder: string[], stateFilter: string, query: string,
  ) => Array<{ state: string; total: number; rootIds(): string[] }>;

  return {
    core_version,

    ticket_matches(ticket, stateFilter, query) {
      const wasmTicket = new TicketSummaryClass(
        ticket.id,
        ticket.type,
        ticket.title ?? '',
        ticket.state ?? '',
      );
      return ticket_matches_fn(wasmTicket, stateFilter, query);
    },

    build_dependency_maps(tickets, edges) {
      const ids = tickets.map(t => t.id);
      const froms = edges.map(e => e.from);
      const tos = edges.map(e => e.to);
      const kinds = edges.map(e => e.kind);

      const maps = WasmDependencyMapsClass.build(ids, froms, tos, kinds);

      const depsOf = new Map<string, string[]>();
      const parentOf = new Map<string, string[]>();

      for (const t of tickets) {
        const deps = Array.from(maps.depsOf(t.id) as string[]);
        if (deps.length > 0) { depsOf.set(t.id, deps); }
        const parents = Array.from(maps.parentOf(t.id) as string[]);
        if (parents.length > 0) { parentOf.set(t.id, parents); }
      }

      return { depsOf, parentOf };
    },

    build_state_groups(tickets, edges, stateOrder, query) {
      const ids = tickets.map(t => t.id);
      const titles = tickets.map(t => t.title ?? '');
      const states = tickets.map(t => t.state ?? '');
      const froms = edges.map(e => e.from);
      const tos = edges.map(e => e.to);
      const kinds = edges.map(e => e.kind);

      const groups = buildStateGroups_fn(
        ids, titles, states, froms, tos, kinds, stateOrder, '', query,
      );

      return Array.from(groups).map(g => ({
        state: g.state,
        total: g.total,
        rootIds: Array.from(g.rootIds()),
      }));
    },

    supports_server_control(hostKind) {
      return supports_server_control_fn(HOST_KIND_MAP[hostKind]);
    },

    supports_browser_bridge(hostKind) {
      return supports_browser_bridge_fn(HOST_KIND_MAP[hostKind]);
    },

    supports_file_browsing(hostKind) {
      return supports_file_browsing_fn(HOST_KIND_MAP[hostKind]);
    },

    ticket_viewer_url(baseUrl, workspace, ticketId) {
      return ticket_viewer_url_fn(baseUrl, workspace, ticketId);
    },

    ticket_display_label(id, title) {
      return ticket_display_label_fn(id, title ?? '');
    },
  };
}
