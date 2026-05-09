use std::{
    collections::BTreeMap,
    path::PathBuf,
};

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
        self.with_store(|store| {
            let mut manifest =
                SpecManifest::new(&input.slug, &input.title, &input.component);
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
            Self::json_result(&json!({
                "status": "ok",
                "id": id,
                "slug": input.slug,
                "title": input.title,
                "component": input.component,
                "state": "draft",
            }))
        })
        .await
    }

    pub(super) async fn spec_get_tool(
        &self,
        input: GetSpecInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            if input.full {
                let (spec, body) =
                    store.get_full(&input.id).map_err(Self::spec_err)?;
                let sections =
                    store.list_sections(&input.id).map_err(Self::spec_err)?;
                Self::json_result(&json!({
                    "status": "ok",
                    "spec": {
                        "id": spec.id,
                        "created_at": spec.created_at,
                        "fields": spec.extra,
                        "code_refs": spec.code_refs,
                    },
                    "body": body,
                    "sections": sections,
                }))
            } else {
                let spec = store.get(&input.id).map_err(Self::spec_err)?;
                Self::json_result(&json!({
                    "status": "ok",
                    "spec": {
                        "id": spec.id,
                        "created_at": spec.created_at,
                        "fields": spec.extra,
                        "code_refs": spec.code_refs,
                    },
                }))
            }
        })
        .await
    }

    pub(super) async fn spec_update_tool(
        &self,
        input: UpdateSpecInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let mut patch = BTreeMap::new();
            for raw in &input.fields {
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
                store.update_body(&input.id, body).map_err(Self::spec_err)?;
            }

            let spec = store
                .update(&input.id, patch, input.to_state.as_deref())
                .map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "id": spec.id,
                "fields": spec.extra,
            }))
        })
        .await
    }

    pub(super) async fn spec_delete_tool(
        &self,
        input: SpecRefInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let id = store.resolve_id(&input.id).map_err(Self::spec_err)?;
            store.delete(&input.id).map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "id": id,
            }))
        })
        .await
    }

    pub(super) async fn spec_list_tool(
        &self,
        input: ListSpecsInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let all = store
                .entity_store()
                .list_indexed(false)
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
            Self::json_result(&json!({
                "status": "ok",
                "count": items.len(),
                "items": items,
            }))
        })
        .await
    }

    pub(super) async fn spec_search_tool(
        &self,
        input: SearchSpecsInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
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
            Self::json_result(&json!({
                "status": "ok",
                "query": input.query,
                "count": items.len(),
                "items": items,
            }))
        })
        .await
    }

    pub(super) async fn spec_tree_tool(
        &self,
        input: TreeInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            if let Some(root_id) = &input.id {
                let root = store.get(root_id).map_err(Self::spec_err)?;
                let descendants =
                    store.subtree(root_id).map_err(Self::spec_err)?;
                Self::json_result(&json!({
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
                }))
            } else {
                let all = store
                    .entity_store()
                    .list_indexed(false)
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
                Self::json_result(&json!({
                    "status": "ok",
                    "roots": roots,
                }))
            }
        })
        .await
    }

    pub(super) async fn spec_health_tool(
        &self,
        input: HealthInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let specs = if input.all {
                let all = store
                    .entity_store()
                    .list_indexed(false)
                    .map_err(Self::storage_err)?;
                all.iter()
                    .filter_map(|entry| store.get(&entry.id.to_string()).ok())
                    .collect::<Vec<_>>()
            } else if let Some(id) = &input.id {
                vec![store.get(id).map_err(Self::spec_err)?]
            } else {
                return Err(McpError::invalid_params(
                    "provide spec ID or set all=true",
                    None,
                ));
            };

            let mut issues = Vec::new();
            for spec in &specs {
                if spec.slug().is_none() {
                    issues
                        .push(json!({"id": spec.id, "issue": "missing slug"}));
                }
                if spec.title().is_none() {
                    issues
                        .push(json!({"id": spec.id, "issue": "missing title"}));
                }
                if spec.component().is_none() {
                    issues.push(
                        json!({"id": spec.id, "issue": "missing component"}),
                    );
                }
            }
            Self::json_result(&json!({
                "status": "ok",
                "specs_checked": specs.len(),
                "issues_count": issues.len(),
                "issues": issues,
            }))
        })
        .await
    }

    pub(super) async fn spec_refs_validate_tool(
        &self,
        input: RefsValidateInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
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
            Self::json_result(&json!({
                "status": "ok",
                "id": spec.id,
                "valid": all_valid,
                "count": items.len(),
                "results": items,
            }))
        })
        .await
    }
}
