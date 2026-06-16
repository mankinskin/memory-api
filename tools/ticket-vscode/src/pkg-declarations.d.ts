// pkg-declarations.d.ts
// Ambient module declaration for the wasm-bindgen background JS module.
// The user-facing types are in pkg/ticket_vscode_core.d.ts (declared via
// the @ts-self-types comment inside ticket_vscode_core.js).
// This file only exposes __wbg_set_wasm which coreLoader.ts needs to wire
// the manually-instantiated WASM binary into the JS glue.
declare module '*/ticket_vscode_core_bg.js' {
  export function __wbg_set_wasm(exports: WebAssembly.Exports): void;
}
