//! Filesystem MCP server.
//!
//! Exposes tools for bounded directory listing and conflict-aware file mutations:
//!
//! - `fs_list_dir`: List directory contents with depth/entry limits and glob patterns.
//! - `fs_stat`: Get file/directory metadata without reading content.
//! - `fs_move_file`: Move a file or directory with conflict detection.
//! - `fs_rename_file`: Rename a file or directory (in-place).
//! - `fs_copy_file`: Copy a file with conflict detection.
//! - `fs_delete_file`: Delete a file.
//! - `fs_delete_dir`: Delete a directory (optionally recursive).

use std::{
    env,
    path::PathBuf,
};

use fs_api::{
    CopyFileRequest,
    DeleteDirRequest,
    DeleteFileRequest,
    FsApiError,
    ListDirRequest,
    ListDirResult,
    MoveFileRequest,
    MutationResult,
    RenameFileRequest,
    StatRequest,
    StatResult,
    copy_file,
    delete_dir,
    delete_file,
    list_dir,
    move_file,
    rename_file,
    stat,
};
use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    ServiceExt,
    handler::server::{
        tool::ToolRouter,
        wrapper::Parameters,
    },
    model::{
        CallToolResult,
        Content,
    },
    schemars::{
        self,
        JsonSchema,
    },
    tool,
    tool_handler,
    tool_router,
    transport::stdio,
};
use serde::{
    Deserialize,
    Serialize,
};

// ── Input types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDirInput {
    /// Root directory to list.
    pub path: PathBuf,

    /// Maximum depth to recurse. Omit to list only the directory itself.
    #[serde(default)]
    pub depth_limit: Option<usize>,

    /// Maximum number of entries to return before truncating.
    #[serde(default)]
    pub entry_limit: Option<usize>,

    /// Glob patterns to include (e.g., "*.rs"). If empty, include all.
    #[serde(default)]
    pub include_globs: Vec<String>,

    /// Glob patterns to exclude (e.g., "target/**", ".git/**").
    #[serde(default)]
    pub exclude_globs: Vec<String>,

    /// Whether to honor .gitignore and other standard filters.
    #[serde(default)]
    pub honor_ignore: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatInput {
    /// Path to the file or directory.
    pub path: PathBuf,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveFileInput {
    /// Source path.
    pub from: PathBuf,

    /// Destination path.
    pub to: PathBuf,

    /// Whether to overwrite existing destination.
    #[serde(default)]
    pub overwrite: bool,

    /// Root directory for security validation. Both source and destination must
    /// remain within this root after resolving symlinks. Defaults to current
    /// working directory if omitted.
    #[serde(default)]
    pub root: Option<PathBuf>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenameFileInput {
    /// Current path.
    pub from: PathBuf,

    /// New name (relative to parent directory).
    pub to: PathBuf,

    /// Root directory for security validation.
    #[serde(default)]
    pub root: Option<PathBuf>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CopyFileInput {
    /// Source path.
    pub from: PathBuf,

    /// Destination path.
    pub to: PathBuf,

    /// Whether to overwrite existing destination.
    #[serde(default)]
    pub overwrite: bool,

    /// Root directory for security validation.
    #[serde(default)]
    pub root: Option<PathBuf>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteFileInput {
    /// Path to the file to delete.
    pub path: PathBuf,

    /// Root directory for security validation.
    #[serde(default)]
    pub root: Option<PathBuf>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteDirInput {
    /// Path to the directory to delete.
    pub path: PathBuf,

    /// Whether to allow deleting non-empty directories.
    #[serde(default)]
    pub recursive: bool,

    /// Root directory for security validation.
    #[serde(default)]
    pub root: Option<PathBuf>,
}

// ── Server ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct FsServer {
    tool_router: ToolRouter<Self>,
}

impl FsServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    fn json_result<T: Serialize>(
        value: &T
    ) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string(value).map_err(|e| {
            McpError::internal_error(format!("serialization: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    async fn list_dir_tool(
        &self,
        input: ListDirInput,
    ) -> Result<CallToolResult, McpError> {
        let request = ListDirRequest {
            path: input.path,
            depth_limit: input.depth_limit,
            entry_limit: input.entry_limit,
            include_globs: input.include_globs,
            exclude_globs: input.exclude_globs,
            honor_ignore: input.honor_ignore,
        };

        let result: ListDirResult =
            tokio::task::spawn_blocking(move || list_dir(&request))
                .await
                .map_err(|e| {
                    McpError::internal_error(format!("task error: {e}"), None)
                })?
                .map_err(|e: FsApiError| {
                    McpError::invalid_params(e.to_string(), None)
                })?;

        Self::json_result(&result)
    }

    async fn stat_tool(
        &self,
        input: StatInput,
    ) -> Result<CallToolResult, McpError> {
        let request = StatRequest { path: input.path };

        let result: StatResult =
            tokio::task::spawn_blocking(move || stat(&request))
                .await
                .map_err(|e| {
                    McpError::internal_error(format!("task error: {e}"), None)
                })?
                .map_err(|e: FsApiError| {
                    McpError::invalid_params(e.to_string(), None)
                })?;

        Self::json_result(&result)
    }

    async fn move_file_tool(
        &self,
        input: MoveFileInput,
    ) -> Result<CallToolResult, McpError> {
        let root = match input.root {
            Some(r) => r,
            None => env::current_dir().map_err(|e| {
                McpError::invalid_params(
                    format!("cannot determine current directory for root validation: {}", e),
                    None,
                )
            })?,
        };
        let request = MoveFileRequest {
            from: input.from,
            to: input.to,
            overwrite: input.overwrite,
            root,
        };

        let result: MutationResult =
            tokio::task::spawn_blocking(move || move_file(&request))
                .await
                .map_err(|e| {
                    McpError::internal_error(format!("task error: {e}"), None)
                })?
                .map_err(|e: FsApiError| {
                    McpError::invalid_params(e.to_string(), None)
                })?;

        Self::json_result(&result)
    }

    async fn rename_file_tool(
        &self,
        input: RenameFileInput,
    ) -> Result<CallToolResult, McpError> {
        let root = match input.root {
            Some(r) => r,
            None => env::current_dir().map_err(|e| {
                McpError::invalid_params(
                    format!("cannot determine current directory for root validation: {}", e),
                    None,
                )
            })?,
        };
        let request = RenameFileRequest {
            from: input.from,
            to: input.to,
            root,
        };

        let result: MutationResult =
            tokio::task::spawn_blocking(move || rename_file(&request))
                .await
                .map_err(|e| {
                    McpError::internal_error(format!("task error: {e}"), None)
                })?
                .map_err(|e: FsApiError| {
                    McpError::invalid_params(e.to_string(), None)
                })?;

        Self::json_result(&result)
    }

    async fn copy_file_tool(
        &self,
        input: CopyFileInput,
    ) -> Result<CallToolResult, McpError> {
        let root = match input.root {
            Some(r) => r,
            None => env::current_dir().map_err(|e| {
                McpError::invalid_params(
                    format!("cannot determine current directory for root validation: {}", e),
                    None,
                )
            })?,
        };
        let request = CopyFileRequest {
            from: input.from,
            to: input.to,
            overwrite: input.overwrite,
            root,
        };

        let result: MutationResult =
            tokio::task::spawn_blocking(move || copy_file(&request))
                .await
                .map_err(|e| {
                    McpError::internal_error(format!("task error: {e}"), None)
                })?
                .map_err(|e: FsApiError| {
                    McpError::invalid_params(e.to_string(), None)
                })?;

        Self::json_result(&result)
    }

    async fn delete_file_tool(
        &self,
        input: DeleteFileInput,
    ) -> Result<CallToolResult, McpError> {
        let root = match input.root {
            Some(r) => r,
            None => env::current_dir().map_err(|e| {
                McpError::invalid_params(
                    format!("cannot determine current directory for root validation: {}", e),
                    None,
                )
            })?,
        };
        let request = DeleteFileRequest {
            path: input.path,
            root,
        };

        let result: MutationResult =
            tokio::task::spawn_blocking(move || delete_file(&request))
                .await
                .map_err(|e| {
                    McpError::internal_error(format!("task error: {e}"), None)
                })?
                .map_err(|e: FsApiError| {
                    McpError::invalid_params(e.to_string(), None)
                })?;

        Self::json_result(&result)
    }

    async fn delete_dir_tool(
        &self,
        input: DeleteDirInput,
    ) -> Result<CallToolResult, McpError> {
        let root = match input.root {
            Some(r) => r,
            None => env::current_dir().map_err(|e| {
                McpError::invalid_params(
                    format!("cannot determine current directory for root validation: {}", e),
                    None,
                )
            })?,
        };
        let request = DeleteDirRequest {
            path: input.path,
            recursive: input.recursive,
            root,
        };

        let result: MutationResult =
            tokio::task::spawn_blocking(move || delete_dir(&request))
                .await
                .map_err(|e| {
                    McpError::internal_error(format!("task error: {e}"), None)
                })?
                .map_err(|e: FsApiError| {
                    McpError::invalid_params(e.to_string(), None)
                })?;

        Self::json_result(&result)
    }
}

// ── MCP tool surface (delegates to impl methods above) ────────────────────────

#[tool_router]
impl FsServer {
    #[tool(description = "
List directory contents with bounded output. Supports depth limits, entry limits,
glob patterns, and .gitignore honor. Returns entries with relative paths, kind
(file/directory/symlink), and size.

Bounded operation: results are truncated if entry_limit is exceeded. Check the
truncated flag and total_found count in the response.
")]
    pub async fn fs_list_dir(
        &self,
        Parameters(input): Parameters<ListDirInput>,
    ) -> Result<CallToolResult, McpError> {
        self.list_dir_tool(input).await
    }

    #[tool(description = "
Get file or directory metadata without reading content. Returns existence status,
kind (file/directory/symlink), size, and modification time.

Use this instead of listing the parent directory when you only need metadata for
a single path.
")]
    pub async fn fs_stat(
        &self,
        Parameters(input): Parameters<StatInput>,
    ) -> Result<CallToolResult, McpError> {
        self.stat_tool(input).await
    }

    #[tool(description = "
Move a file or directory to a new location. Conflict detection: reports
DestinationExists unless overwrite is true.

The operation is atomic on most filesystems. Use this for relocating files
across directories.
")]
    pub async fn fs_move_file(
        &self,
        Parameters(input): Parameters<MoveFileInput>,
    ) -> Result<CallToolResult, McpError> {
        self.move_file_tool(input).await
    }

    #[tool(description = "
Rename a file or directory in-place (same parent directory). The 'to' path should
be just the new name, not a full path.

Use move_file for relocating across directories.
")]
    pub async fn fs_rename_file(
        &self,
        Parameters(input): Parameters<RenameFileInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rename_file_tool(input).await
    }

    #[tool(description = "
Copy a file to a new location. Conflict detection: reports DestinationExists
unless overwrite is true.

Does not copy directories — use list_dir and copy individual files for directory
copies.
")]
    pub async fn fs_copy_file(
        &self,
        Parameters(input): Parameters<CopyFileInput>,
    ) -> Result<CallToolResult, McpError> {
        self.copy_file_tool(input).await
    }

    #[tool(description = "
Delete a file. Reports SourceMissing if the file does not exist.

Use delete_dir for directories.
")]
    pub async fn fs_delete_file(
        &self,
        Parameters(input): Parameters<DeleteFileInput>,
    ) -> Result<CallToolResult, McpError> {
        self.delete_file_tool(input).await
    }

    #[tool(description = "
Delete a directory. Set recursive=true to delete non-empty directories.

Without recursive, reports an error if the directory is not empty.
")]
    pub async fn fs_delete_dir(
        &self,
        Parameters(input): Parameters<DeleteDirInput>,
    ) -> Result<CallToolResult, McpError> {
        self.delete_dir_tool(input).await
    }
}

impl Default for FsServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_handler]
impl ServerHandler for FsServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            server_info: rmcp::model::Implementation {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            instructions: Some(
                "Filesystem MCP. Exposes bounded directory listing and conflict-aware \
                 file mutations (list_dir, stat, move_file, rename_file, copy_file, \
                 delete_file, delete_dir)."
                    .into(),
            ),
            capabilities: rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        }
    }
}

pub async fn run_mcp_server() -> anyhow::Result<()> {
    let server = FsServer::new();
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
