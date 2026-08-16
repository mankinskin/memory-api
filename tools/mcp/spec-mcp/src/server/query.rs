use std::path::PathBuf;

use rmcp::{
    ErrorData as McpError,
    model::CallToolResult,
};
use serde_json::{
    Value,
    json,
};

use spec_api::{
    SpecManifest,
    code_ref::validate_refs,
};

use super::{
    CreateSpecInput,
    GetSpecInput,
    HealthInput,
    ListSpecsInput,
    RefsValidateInput,
    SearchSpecsInput,
    SpecRefInput,
    SpecServer,
    TreeInput,
    UpdateSpecInput,
};

impl SpecServer {
    pub(super) async fn spec_create_tool(
        &self,
        input: CreateSpecInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace =
            memory_kernel::workspace::validate_explicit_workspace_selector(
                Some(&input.workspace),
            )
            .map_err(|err| McpError::invalid_params(err.to_string(), None))?
            .to_string();
        self.with_store(Some(&workspace), |store, index_root| {
            let mut manifest =
                SpecManifest::new(&input.slug, &input.title, &input.component);
            manifest.extra.extend(input.fields.clone());
            manifest.set_slug(&input.slug);
            manifest.set_title(&input.title);
            manifest.set_component(&input.component);
            if let Some(parent) = &input.parent {
                let parent_id =
                    store.resolve_id(parent).map_err(Self::spec_err)?;
                manifest.set_parent(&parent_id.to_string());
            }
            if let Some(scope) = &input.scope {
                manifest.set_scope(scope);
            }
            let body = input.body.as_deref().unwrap_or("");
            let id = store
                .create(&manifest, body, None)
                .map_err(Self::spec_err)?;
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "id": id,
                    "slug": input.slug,
                    "title": input.title,
                    "component": input.component,
                    "state": "draft",
                }),
                index_root,
                Some(&workspace),
            )
        })
        .await
    }

    pub(super) async fn spec_get_tool(
        &self,
        input: GetSpecInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            if input.full {
                let (spec, body) =
                    store.get_full(&input.id).map_err(Self::spec_err)?;
                let sections =
                    store.list_sections(&input.id).map_err(Self::spec_err)?;
                Self::json_result_with_scope(
                    json!({
                        "status": "ok",
                        "spec": {
                            "id": spec.id,
                            "created_at": spec.created_at,
                            "fields": spec.extra,
                            "code_refs": spec.code_refs,
                        },
                        "body": body,
                        "sections": sections,
                    }),
                    index_root,
                    workspace.as_deref(),
                )
            } else {
                let spec = store.get(&input.id).map_err(Self::spec_err)?;
                Self::json_result_with_scope(
                    json!({
                        "status": "ok",
                        "spec": {
                            "id": spec.id,
                            "created_at": spec.created_at,
                            "fields": spec.extra,
                            "code_refs": spec.code_refs,
                        },
                    }),
                    index_root,
                    workspace.as_deref(),
                )
            }
        })
        .await
    }

    pub(super) async fn spec_update_tool(
        &self,
        input: UpdateSpecInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            let previous = store.get(&input.id).map_err(Self::spec_err)?;
            let mut patch = input.field_map.clone().unwrap_or_default();
            for raw in input.fields.clone().unwrap_or_default() {
                let (key, value) = raw.split_once('=').ok_or_else(|| {
                    McpError::invalid_params(
                        format!(
                            "invalid field format '{raw}', expected key=value"
                        ),
                        None,
                    )
                })?;
                patch.insert(
                    key.trim().to_string(),
                    Value::String(value.trim().to_string()),
                );
            }

            if let Some(body) = &input.body {
                store
                    .update_body(&input.id, body, input.force_body)
                    .map_err(Self::spec_err)?;
            }

            let changed_fields = patch.clone();
            let spec = store
                .update(&input.id, patch, input.to_state.as_deref())
                .map_err(Self::spec_err)?;
            let mut response = serde_json::Map::from_iter([
                ("status".to_string(), Value::String("ok".to_string())),
                ("id".to_string(), json!(spec.id)),
            ]);
            if !changed_fields.is_empty() {
                response.insert(
                    "changed_fields".to_string(),
                    Value::Object(changed_fields.into_iter().collect()),
                );
            }
            if let Some(to_state) = input.to_state {
                response.insert(
                    "state_transition".to_string(),
                    json!({
                        "from": previous.state(),
                        "to": to_state,
                    }),
                );
            }
            if input.body.is_some() {
                response.insert("body_updated".to_string(), Value::Bool(true));
            }
            Self::json_result_with_scope(
                Value::Object(response),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }

    pub(super) async fn spec_delete_tool(
        &self,
        input: SpecRefInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            let id = store.resolve_id(&input.id).map_err(Self::spec_err)?;
            store.delete(&input.id).map_err(Self::spec_err)?;
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "id": id,
                }),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }

    pub(super) async fn spec_list_tool(
        &self,
        input: ListSpecsInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            let all = store
                .entity_store()
                .list_indexed()
                .map_err(Self::storage_err)?;
            let mut items: Vec<Value> = Vec::new();
            'outer: for indexed in &all {
                let spec = match store.get(&indexed.id.to_string()) {
                    Ok(spec) => spec,
                    Err(_) => continue,
                };
                for clause in &input.where_clauses {
                    if let Some((key, value)) = clause.split_once('=') {
                        let field_val = spec
                            .extra
                            .get(key)
                            .and_then(|field| field.as_str());
                        if field_val != Some(value) {
                            continue 'outer;
                        }
                    }
                }
                items.push(json!({
                    "id": indexed.id,
                    "slug": spec.slug(),
                    "title": spec.title(),
                    "state": spec.state(),
                    "component": spec.component(),
                }));
                if let Some(limit) = input.limit {
                    if items.len() >= limit {
                        break;
                    }
                }
            }
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "count": items.len(),
                    "items": items,
                }),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }

    pub(super) async fn spec_search_tool(
        &self,
        input: SearchSpecsInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            let results = store
                .entity_store()
                .search(&input.query, input.limit)
                .map_err(Self::storage_err)?;
            let items: Vec<Value> = results
                .iter()
                .map(|result| {
                    json!({
                        "id": result.id,
                        "title": result.title,
                        "state": result.state,
                        "type": result.ticket_type,
                        "score": result.score,
                        "snippet": result.snippet,
                    })
                })
                .collect();
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "query": input.query,
                    "count": items.len(),
                    "items": items,
                }),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }

    pub(super) async fn spec_tree_tool(
        &self,
        input: TreeInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            if let Some(root_id) = &input.id {
                let root = store.get(root_id).map_err(Self::spec_err)?;
                let descendants =
                    store.subtree(root_id).map_err(Self::spec_err)?;
                Self::json_result_with_scope(
                    json!({
                        "status": "ok",
                        "root": {
                            "id": root.id,
                            "slug": root.slug(),
                            "title": root.title(),
                            "state": root.state(),
                        },
                        "descendants": descendants.iter().map(|child| json!({
                            "id": child.id,
                            "slug": child.slug(),
                            "title": child.title(),
                            "state": child.state(),
                            "parent": child.parent(),
                        })).collect::<Vec<_>>(),
                    }),
                    index_root,
                    workspace.as_deref(),
                )
            } else {
                let all = store
                    .entity_store()
                    .list_indexed()
                    .map_err(Self::storage_err)?;
                let mut roots = Vec::new();
                for indexed in &all {
                    if let Ok(spec) = store.get(&indexed.id.to_string()) {
                        if spec.parent().is_none() {
                            let children = store
                                .children(&indexed.id.to_string())
                                .map_err(Self::spec_err)?;
                            roots.push(json!({
                                "id": spec.id,
                                "slug": spec.slug(),
                                "title": spec.title(),
                                "children_count": children.len(),
                            }));
                        }
                    }
                }
                Self::json_result_with_scope(
                    json!({
                        "status": "ok",
                        "roots": roots,
                    }),
                    index_root,
                    workspace.as_deref(),
                )
            }
        })
        .await
    }

    pub(super) async fn spec_health_tool(
        &self,
        input: HealthInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            let report = if input.all {
                store.health_all().map_err(Self::spec_err)?
            } else if let Some(id) = &input.id {
                store.health(id).map_err(Self::spec_err)?
            } else {
                return Err(McpError::invalid_params(
                    "provide spec ID or set all=true",
                    None,
                ));
            };
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "specs_checked": report.specs_checked,
                    "issues_count": report.issues_count(),
                    "issues": report.issues,
                }),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }

    pub(super) async fn spec_refs_validate_tool(
        &self,
        input: RefsValidateInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            let spec = store.get(&input.id).map_err(Self::spec_err)?;
            let workspace_root = PathBuf::from(&input.workspace_root);
            let results = validate_refs(&spec.code_refs, &workspace_root);
            let items: Vec<Value> = results
                .iter()
                .map(|result| {
                    json!({
                        "file": result.code_ref.file,
                        "symbol": result.code_ref.symbol,
                        "kind": format!("{:?}", result.code_ref.kind),
                        "file_exists": result.file_exists,
                        "line_range_valid": result.line_range_valid,
                        "message": result.message,
                    })
                })
                .collect();
            let all_valid = results
                .iter()
                .all(|result| result.file_exists && result.line_range_valid);
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "id": spec.id,
                    "valid": all_valid,
                    "count": items.len(),
                    "results": items,
                }),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
    };

    use serde::Deserialize;
    use spec_api::SpecStore;
    use tempfile::tempdir;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct ContractParityFixture {
        fields: BTreeMap<String, Value>,
        fulfillment_update: BTreeMap<String, Value>,
        search_query: String,
        expected_health_issue: String,
    }

    fn load_contract_parity_fixture() -> ContractParityFixture {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../test-fixtures/spec-contract-parity.json");
        serde_json::from_str(&fs::read_to_string(fixture_path).unwrap())
            .unwrap()
    }

    fn parse_tool_payload(result: CallToolResult) -> Value {
        let encoded = serde_json::to_value(result).unwrap();
        let text = encoded["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap()
    }

    #[tokio::test]
    async fn mcp_tools_round_trip_structured_contract_fields() {
        let dir = tempdir().unwrap();
        let index_root = dir.path().join(".spec");
        SpecStore::init(&index_root).unwrap();
        let server = SpecServer::new(index_root.clone());
        let fixture = load_contract_parity_fixture();

        let created = parse_tool_payload(
            server
                .spec_create_tool(CreateSpecInput {
                    workspace: index_root.display().to_string(),
                    title: "Structured contract parity spec".to_string(),
                    slug: "contract/structured-parity".to_string(),
                    component: "context-engine".to_string(),
                    parent: None,
                    scope: Some("public".to_string()),
                    body: None,
                    fields: fixture.fields.clone(),
                })
                .await
                .unwrap(),
        );

        let spec_id = created["id"].as_str().unwrap().to_string();

        let health_before = parse_tool_payload(
            server
                .spec_health_tool(HealthInput {
                    workspace: None,
                    id: Some(spec_id.clone()),
                    all: false,
                })
                .await
                .unwrap(),
        );

        assert_eq!(health_before["issues_count"], 1);
        assert_eq!(
            health_before["issues"][0]["issue"],
            fixture.expected_health_issue
        );

        parse_tool_payload(
            server
                .spec_update_tool(UpdateSpecInput {
                    workspace: None,
                    id: spec_id.clone(),
                    fields: None,
                    to_state: None,
                    body: None,
                    force_body: false,
                    field_map: Some(fixture.fulfillment_update.clone()),
                })
                .await
                .unwrap(),
        );

        let fetched = parse_tool_payload(
            server
                .spec_get_tool(GetSpecInput {
                    workspace: None,
                    id: spec_id.clone(),
                    full: false,
                })
                .await
                .unwrap(),
        );

        assert_eq!(
            fetched["spec"]["fields"]["contract_mode"],
            "expectation-oriented"
        );
        assert_eq!(
            fetched["spec"]["fields"]["fulfillment_summaries"][0]["status"],
            "satisfied"
        );

        let searched = parse_tool_payload(
            server
                .spec_search_tool(SearchSpecsInput {
                    workspace: None,
                    query: fixture.search_query,
                    limit: 10,
                })
                .await
                .unwrap(),
        );

        assert_eq!(searched["count"], 1);
        assert_eq!(searched["items"][0]["id"], spec_id);

        let health_after = parse_tool_payload(
            server
                .spec_health_tool(HealthInput {
                    workspace: None,
                    id: Some(spec_id),
                    all: false,
                })
                .await
                .unwrap(),
        );

        assert_eq!(health_after["issues_count"], 0);
    }
}
