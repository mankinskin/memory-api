## Objective

Wire the `pdf-mcp` binary transport following the canonical named-tool MCP pattern from `memory-api/tools/mcp/peek-mcp/src/server.rs`, exposing every `pdf-api` operation as a named MCP tool.

## Target Files

- `memory-api/crates/pdf/src/bin/pdf-mcp.rs`
- `memory-api/crates/pdf/Cargo.toml` (confirm `mcp` feature deps: `rmcp`, `schemars`, `serde`, `serde_json` alongside `transport-harness`)

## Design

Mirror `memory-api/tools/mcp/peek-mcp/src/server.rs` exactly in structure:

- One `#[derive(Debug, Deserialize, JsonSchema)] pub struct <Op>Input` per operation (e.g. `PdfExtractTextInput`, `PdfMergeInput`, `PdfSplitInput`, `PdfReorderPagesInput`, `PdfDeletePagesInput`, `PdfReadMetadataInput`, `PdfWriteMetadataInput`, `PdfCreateInput`, `PdfCreateFromTypstInput`, `PdfExtractImagesInput`), each carrying whatever fields the corresponding `PdfRequest` variant needs, plus a `root: String` (or `PathBuf`) confinement field on every input (mirroring how `fs-mcp`-family tools take `root` — confirm the exact convention used by an existing root-confined MCP tool in this repo, e.g. `mcp_fs-mcp_fs_list_dir`'s `root` parameter, and match it).
- `#[derive(Clone)] pub struct PdfServer { tool_router: ToolRouter<Self> }` with `pub fn new() -> Self`.
- A `json_result<T: Serialize>(value: &T) -> Result<CallToolResult, McpError>` helper identical in shape to peek-mcp's.
- An error-mapper converting `PdfError` variants to `McpError`: user-error variants (bad path, root escape, missing overwrite flag, out-of-range page index, file not found) map to `McpError::invalid_params`; everything else (corrupt PDF, unexpected crate failure, typst-cli process failure) maps to `McpError::internal_error`.
- `#[tool_router] impl PdfServer` with one `#[tool(name = "...", description = "...")] async fn` per operation, each taking `Parameters<XInput>`, calling `pdf_api::execute`, and returning via `json_result`/the error mapper.
- `#[tool_handler] impl ServerHandler for PdfServer` setting `server_info` from `env!("CARGO_PKG_NAME")`/`env!("CARGO_PKG_VERSION")`, an `instructions` string describing the PDF toolset and pointing agents to prefer these named tools over `pdf-cli`, and `ServerCapabilities::builder().enable_tools().build()`.
- `pdf-mcp.rs`'s `main` calls a `run_mcp_server()`-equivalent that does `PdfServer::new().serve(stdio()).await?.waiting().await?;` per the peek-mcp precedent.

## Acceptance Criteria

- [ ] `pdf-mcp` exposes one named tool per `pdf-api` operation (text extraction, image extraction, merge, split, reorder, delete, read-metadata, write-metadata, create-programmatic, create-from-typst).
- [ ] Every tool input struct includes an explicit confinement root field; no tool defaults or omits it.
- [ ] Every output-producing tool input includes an explicit `output` path and `overwrite` flag mirroring T2's write-safety contract.
- [ ] `PdfError` user-error variants map to `McpError::invalid_params`; internal/unexpected failures map to `McpError::internal_error` (unit or integration test confirms at least one case of each mapping).
- [ ] `server_info` reports the correct package name/version and an `instructions` string mentioning PDF operations.
- [ ] `cargo build -p pdf --features mcp` succeeds and produces a working `pdf-mcp` binary that starts an MCP stdio server.
- [ ] Tool-router registration includes all operations (no operation from `PdfRequest` is left unexposed as an MCP tool, except none are expected to be withheld — confirm 1:1 coverage).

## Validation Plan

```bash
cargo build -p pdf --features mcp
```
Manual MCP client smoke test (e.g. via an MCP inspector or a short-lived stdio session) calling the extract-text tool against a T3 fixture and confirming the expected text is returned, plus one deliberate error case (bad root) confirming `invalid_params` is returned rather than a crash.