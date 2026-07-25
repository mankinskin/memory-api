use serde::Deserialize;
use transport_harness::mcp::rmcp::{
    self as rmcp,
    ErrorData as McpError,
    ServerHandler,
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
};

#[derive(Clone)]
struct TicketServer {
    tool_router: ToolRouter<Self>,
}

impl TicketServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GetArgs {
    /// Ticket ID to retrieve.
    id: String,
    /// Store path.
    store_path: String,
}

#[tool_router]
impl TicketServer {
    /// Get a ticket by ID.
    #[tool(description = "Get a ticket by ID")]
    async fn get(
        &self,
        Parameters(args): Parameters<GetArgs>,
    ) -> Result<CallToolResult, McpError> {
        let store = ticket::storage::TicketStore::open(std::path::Path::new(&args.store_path))
            .map_err(|e| McpError::invalid_params(format!("failed to open store: {e}"), None))?;
        
        let uuid = args.id.parse::<uuid::Uuid>()
            .map_err(|e| McpError::invalid_params(format!("invalid ticket id: {e}"), None))?;
        
        let ticket = store
            .get(&uuid)
            .map_err(|e| McpError::invalid_params(format!("ticket not found: {e}"), None))?;
        
        let json = serde_json::to_string(&ticket)
            .map_err(|e| McpError::internal_error(format!("failed to serialize ticket: {e}"), None))?;
        
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl ServerHandler for TicketServer {}

fn main() {
    let server = TicketServer::new();
    if let Err(err) = transport_harness::mcp::run(server) {
        eprintln!("Fatal error: {err}");
        std::process::exit(1);
    }
}
