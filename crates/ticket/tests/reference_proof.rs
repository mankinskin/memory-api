//! Reference-proof integration tests for the ticket domain.
//!
//! Mirrors the transport-harness reference-proof pattern: exercises one
//! realistic domain operation (`get`) across CLI, MCP, and HTTP, proving both
//! success output shape and error handling (including HTTP status mapping).

/// Fixture domain: sets up a temporary ticket store with known test data.
///
/// Only compiled when a transport feature is active; under `default = []` this
/// module is absent, preserving slimness.
#[cfg(any(feature = "cli", feature = "mcp", feature = "http"))]
mod fixture {
    use std::path::PathBuf;
    use tempfile::TempDir;
    use ticket::storage::TicketStore;

    /// Test fixture with a known ticket in a temporary store.
    pub struct TestStore {
        pub temp_dir: TempDir,
        pub store_path: PathBuf,
        pub test_ticket_id: String,
    }

    impl TestStore {
        /// Creates a temporary ticket store with one known test ticket.
        pub fn setup() -> Self {
            let temp_dir = tempfile::tempdir().expect("temp dir");
            let store_path = temp_dir.path().join(".ticket");
            
            // Initialize the ticket store
            ticket::storage::TicketStore::init(&store_path).expect("init store");

            let test_ticket_id = "aaaa0000-0000-0000-0000-000000000001";
            memory_fixtures::append_fixture_ticket(
                &store_path,
                test_ticket_id,
                "Test Ticket",
                "open",
                "core",
            )
            .expect("write ticket");

            Self {
                temp_dir,
                store_path: store_path.clone(),
                test_ticket_id: test_ticket_id.to_string(),
            }
        }

        /// Opens the test store.
        pub fn open(&self) -> TicketStore {
            TicketStore::open(&self.store_path).expect("open store")
        }
    }
}

/// CLI transport: parses a domain subcommand and dispatches through shared
/// output and error handling.
#[cfg(feature = "cli")]
mod cli_proof {
    use transport_harness::{
        HarnessError,
        Output,
        cli::{
            self,
            clap::{
                self,
                Parser,
            },
        },
    };

    use super::fixture::TestStore;

    #[derive(Parser)]
    #[command(name = "ticket-cli")]
    struct TicketCommand {
        #[command(subcommand)]
        op: Op,
    }

    #[derive(clap::Subcommand)]
    enum Op {
        Get {
            #[arg(long)]
            id: String,
            #[arg(long)]
            store_path: String,
        },
    }

    fn dispatch(command: TicketCommand) -> Result<Output, HarnessError> {
        match command.op {
            Op::Get { id, store_path } => {
                let store = ticket::storage::TicketStore::open(std::path::Path::new(&store_path))
                    .map_err(|e| HarnessError::domain(format!("failed to open store: {e}")))?;
                
                let id = id.parse::<uuid::Uuid>()
                    .map_err(|e| HarnessError::domain(format!("invalid UUID: {e}")))?;
                
                let ticket = store
                    .get(&id)
                    .map_err(|e| HarnessError::domain(format!("ticket not found: {e}")))?;
                
                Output::json(&ticket)
            }
        }
    }

    #[test]
    fn cli_get_success_emits_one_json_line() {
        let test_store = TestStore::setup();
        let mut buffer = Vec::new();
        
        cli::run_from(
            [
                "ticket-cli",
                "get",
                "--id",
                &test_store.test_ticket_id,
                "--store-path",
                &test_store.store_path.display().to_string(),
            ],
            &mut buffer,
            dispatch,
        )
        .expect("cli dispatch should succeed");
        
        let output = String::from_utf8(buffer).expect("valid utf8");
        assert!(output.contains(&test_store.test_ticket_id));
        assert!(output.contains("Test Ticket"));
    }

    #[test]
    fn cli_get_unknown_id_returns_domain_error() {
        let test_store = TestStore::setup();
        let mut buffer = Vec::new();
        
        let error = cli::run_from(
            [
                "ticket-cli",
                "get",
                "--id",
                "00000000-0000-0000-0000-000000000000",
                "--store-path",
                &test_store.store_path.display().to_string(),
            ],
            &mut buffer,
            dispatch,
        )
        .expect_err("unknown id should fail");
        
        assert!(matches!(error, HarnessError::Domain(_)));
        assert!(error.to_string().contains("ticket not found"));
    }
}

/// MCP transport: registers a domain tool and invokes it in-process for both
/// the success and the domain-error paths.
#[cfg(feature = "mcp")]
mod mcp_proof {
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
            RawContent,
        },
        schemars::{
            self,
            JsonSchema,
        },
        tool,
        tool_handler,
        tool_router,
    };

    use super::fixture::TestStore;

    #[derive(Clone)]
    struct TestTicketServer {
        tool_router: ToolRouter<Self>,
        store_path: String,
    }

    impl TestTicketServer {
        fn new(store_path: String) -> Self {
            Self {
                tool_router: Self::tool_router(),
                store_path,
            }
        }
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    struct GetArgs {
        id: String,
    }

    #[tool_router]
    impl TestTicketServer {
        #[tool(description = "Get a ticket by ID")]
        async fn get(
            &self,
            Parameters(args): Parameters<GetArgs>,
        ) -> Result<CallToolResult, McpError> {
            let store = ticket::storage::TicketStore::open(std::path::Path::new(&self.store_path))
                .map_err(|e| McpError::invalid_params(format!("failed to open store: {e}"), None))?;
            
            let id = args.id.parse::<uuid::Uuid>()
                .map_err(|e| McpError::invalid_params(format!("invalid UUID: {e}"), None))?;
            
            let ticket = store
                .get(&id)
                .map_err(|e| McpError::invalid_params(format!("ticket not found: {e}"), None))?;
            
            let json = serde_json::to_string(&ticket)
                .expect("ticket should serialize");
            
            Ok(CallToolResult::success(vec![Content::text(json)]))
        }
    }

    #[tool_handler]
    impl ServerHandler for TestTicketServer {}

    fn text_of(result: &CallToolResult) -> String {
        let content = result.content.first().expect("result should carry content");
        match &content.raw {
            RawContent::Text(text) => text.text.clone(),
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn mcp_get_tool_success() {
        let test_store = TestStore::setup();
        let server = TestTicketServer::new(test_store.store_path.display().to_string());
        
        let result = server
            .get(Parameters(GetArgs {
                id: test_store.test_ticket_id.clone(),
            }))
            .await
            .expect("tool call should succeed");
        
        let output = text_of(&result);
        assert!(output.contains(&test_store.test_ticket_id));
        assert!(output.contains("Test Ticket"));
    }

    #[tokio::test]
    async fn mcp_get_tool_unknown_id_errors() {
        let test_store = TestStore::setup();
        let server = TestTicketServer::new(test_store.store_path.display().to_string());
        
        let error = server
            .get(Parameters(GetArgs {
                id: "00000000-0000-0000-0000-000000000000".to_string(),
            }))
            .await
            .expect_err("unknown id should error");
        
        assert!(format!("{error:?}").contains("ticket not found"));
    }
}

/// HTTP transport: registers a domain success route and a domain error route
/// that maps through the shared `HttpError` envelope and status code.
#[cfg(feature = "http")]
mod http_proof {
    use tower::ServiceExt;
    use transport_harness::http::{
        HttpError,
        Router,
        StatusCode,
        axum::{
            Json,
            body::{
                Body,
                to_bytes,
            },
            extract::Path,
            http::Request,
            response::{
                IntoResponse,
                Response,
            },
            routing::get,
        },
    };

    use super::fixture::TestStore;

    async fn get_ticket(
        Path((store_path, id)): Path<(String, String)>,
    ) -> Response {
        let store = match ticket::storage::TicketStore::open(std::path::Path::new(&store_path)) {
            Ok(store) => store,
            Err(e) => {
                return HttpError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    format!("failed to open store: {e}"),
                )
                .into_response()
            }
        };
        
        let id = match id.parse::<uuid::Uuid>() {
            Ok(id) => id,
            Err(e) => {
                return HttpError::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_id",
                    format!("invalid UUID: {e}"),
                )
                .into_response()
            }
        };
        
        match store.get(&id) {
            Ok(ticket) => Json(ticket).into_response(),
            Err(e) => HttpError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                format!("ticket not found: {e}"),
            )
            .into_response(),
        }
    }

    fn test_router() -> Router {
        Router::new().route("/ticket/{store_path}/{id}", get(get_ticket))
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        serde_json::from_slice(&bytes).expect("body should be json")
    }

    #[tokio::test]
    async fn http_get_success_returns_ticket() {
        let test_store = TestStore::setup();
        let store_path_str = test_store.store_path.display().to_string();
        let store_path_encoded = urlencoding::encode(&store_path_str);
        
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri(format!("/ticket/{}/{}", store_path_encoded, test_store.test_ticket_id))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["id"], test_store.test_ticket_id);
        assert_eq!(json["title"], "Test Ticket");
    }

    #[tokio::test]
    async fn http_get_unknown_id_maps_to_not_found_envelope() {
        let test_store = TestStore::setup();
        let store_path_str = test_store.store_path.display().to_string();
        let store_path_encoded = urlencoding::encode(&store_path_str);
        
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri(format!("/ticket/{}/00000000-0000-0000-0000-000000000000", store_path_encoded))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let json = body_json(response).await;
        assert_eq!(json["code"], "not_found");
        assert!(json["message"].as_str().unwrap().contains("ticket not found"));
    }
}
