//! Schema endpoint handlers.
//!
//! `GET /api/schema?workspace=<name>` — list all registered ticket type schemas.
//! `GET /api/schema/{type_id}?workspace=<name>` — single type schema.

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use std::collections::BTreeMap;

use viewer_api::error::RequestIdExt;
use crate::serve::AppState;

#[derive(serde::Deserialize)]
pub struct SchemaQuery {
    pub workspace: String,
}

// ── Wire-format types ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct FieldDef {
    pub field_type: String,
    pub required: bool,
}

#[derive(Serialize)]
pub struct TransitionDef {
    pub from: String,
    pub to: String,
}

#[derive(Serialize)]
pub struct EdgeRuleDef {
    pub directed: bool,
    pub acyclic_enforced: bool,
}

#[derive(Serialize)]
pub struct TypeSchema {
    pub type_id: String,
    pub states: Vec<String>,
    pub transitions: Vec<TransitionDef>,
    pub fields: BTreeMap<String, FieldDef>,
    pub edge_rules: BTreeMap<String, EdgeRuleDef>,
    pub required_states: Vec<String>,
    pub terminal_states: Vec<String>,
}

#[derive(Serialize)]
pub struct SchemaListResponse {
    pub request_id: String,
    pub workspace: String,
    pub types: Vec<TypeSchema>,
}

#[derive(Serialize)]
pub struct SchemaDetailResponse {
    pub request_id: String,
    pub workspace: String,
    pub schema: TypeSchema,
}

// ── Conversion helpers ────────────────────────────────────────────────────────

fn schema_to_wire(s: &ticket_api::model::schema::TicketTypeSchema) -> TypeSchema {
    use ticket_api::model::schema::FieldType;

    let fields = s
        .fields
        .iter()
        .map(|(name, def)| {
            let field_type = match def.field_type {
                FieldType::String => "string",
                FieldType::Integer => "integer",
                FieldType::Float => "float",
                FieldType::Boolean => "boolean",
                FieldType::DateTime => "datetime",
                FieldType::Json => "json",
            }
            .to_string();
            (
                name.clone(),
                FieldDef {
                    field_type,
                    required: def.required,
                },
            )
        })
        .collect();

    let transitions = s
        .transitions
        .iter()
        .map(|t| TransitionDef {
            from: t.from.clone(),
            to: t.to.clone(),
        })
        .collect();

    let edge_rules = s
        .edge_rules
        .iter()
        .map(|(kind, rule)| {
            (
                kind.clone(),
                EdgeRuleDef {
                    directed: rule.directed,
                    acyclic_enforced: rule.acyclic_enforced,
                },
            )
        })
        .collect();

    TypeSchema {
        type_id: s.type_id.clone(),
        states: s.states.clone(),
        transitions,
        fields,
        edge_rules,
        required_states: s.required_states.clone(),
        terminal_states: s.terminal_states.clone(),
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /api/schema?workspace=<name>`
///
/// Returns all ticket type schemas registered in the given workspace.
pub async fn list_schemas(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<SchemaQuery>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let registry = store.schema_registry();
    let types: Vec<TypeSchema> = registry
        .type_ids()
        .filter_map(|id| registry.get(id))
        .map(schema_to_wire)
        .collect();

    Json(SchemaListResponse {
        request_id: rid.0,
        workspace: params.workspace,
        types,
    })
    .into_response()
}

/// `GET /api/schema/{type_id}?workspace=<name>`
///
/// Returns the schema for a single ticket type.  Returns 404 if the type is
/// not registered in the workspace schema registry.
pub async fn get_schema(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(type_id): Path<String>,
    Query(params): Query<SchemaQuery>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let registry = store.schema_registry();
    match registry.get(&type_id) {
        Some(schema) => Json(SchemaDetailResponse {
            request_id: rid.0,
            workspace: params.workspace,
            schema: schema_to_wire(schema),
        })
        .into_response(),
        None => viewer_api::error::ApiError::not_found("schema", &rid.0)
            .into_response_with_status(StatusCode::NOT_FOUND),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{get_schema, list_schemas, SchemaQuery};
    use axum::{
        body::to_bytes,
        extract::{Extension, Path, Query, State},
        http::StatusCode,
    };
    use std::sync::Arc;
    use ticket_api::{model::filesystem::ScanRoot, storage::store::TicketStore};
    use viewer_api::error::RequestIdExt;

    use crate::serve::{AppState, StreamBroker, WorkspaceRegistry};

    fn make_state(dir: &std::path::Path) -> AppState {
        let store = Arc::new(TicketStore::open(dir).expect("open store"));
        store
            .add_scan_root(ScanRoot {
                path: dir.join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");
        AppState::new(
            Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&store))),
            Arc::new(StreamBroker::new()),
        )
    }

    #[tokio::test]
    async fn list_schemas_returns_tracker_improvement() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = make_state(dir.path());

        let response = list_schemas(
            State(state),
            Extension(RequestIdExt("rid-schema".to_string())),
            Query(SchemaQuery {
                workspace: "default".to_string(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

        assert_eq!(payload["workspace"], "default");
        assert_eq!(payload["request_id"], "rid-schema");

        let types = payload["types"].as_array().expect("types array");
        assert!(!types.is_empty(), "should have at least one type");

        let tracker = types
            .iter()
            .find(|t| t["type_id"] == "tracker-improvement")
            .expect("tracker-improvement type present");

        // required_states must contain "in-review"
        let req_states = tracker["required_states"]
            .as_array()
            .expect("required_states array");
        assert!(
            req_states.iter().any(|s| s == "in-review"),
            "required_states must include in-review"
        );

        // terminal_states must contain "done"
        let term_states = tracker["terminal_states"]
            .as_array()
            .expect("terminal_states array");
        assert!(
            term_states.iter().any(|s| s == "done"),
            "terminal_states must include done"
        );

        // states list must be non-empty
        assert!(
            tracker["states"].as_array().expect("states").len() > 0,
            "states list non-empty"
        );

        // fields must contain "title"
        assert!(
            tracker["fields"]["title"].is_object(),
            "fields.title present"
        );
    }

    #[tokio::test]
    async fn get_schema_returns_single_type() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = make_state(dir.path());

        let response = get_schema(
            State(state),
            Extension(RequestIdExt("rid-single".to_string())),
            Path("tracker-improvement".to_string()),
            Query(SchemaQuery {
                workspace: "default".to_string(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

        assert_eq!(payload["schema"]["type_id"], "tracker-improvement");
        assert!(payload["schema"]["fields"]["title"].is_object());

        let req_states = payload["schema"]["required_states"]
            .as_array()
            .expect("required_states");
        assert!(req_states.iter().any(|s| s == "in-review"));
    }

    #[tokio::test]
    async fn get_schema_unknown_type_returns_404() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = make_state(dir.path());

        let response = get_schema(
            State(state),
            Extension(RequestIdExt("rid-miss".to_string())),
            Path("no-such-type".to_string()),
            Query(SchemaQuery {
                workspace: "default".to_string(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
