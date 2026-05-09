use std::collections::BTreeMap;

use rmcp::{
    ErrorData as McpError,
    model::CallToolResult,
};
use serde_json::{
    Value,
    json,
};

use rule_api::{
    RuleFilter,
    RuleManifest,
};

use super::{
    CreateRuleInput,
    ListRulesInput,
    RuleRefInput,
    RuleServer,
    SearchRulesInput,
    UpdateRuleInput,
};

impl RuleServer {
    pub(super) async fn rule_create_tool(
        &self,
        input: CreateRuleInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let mut manifest = RuleManifest::new(
                &input.slug,
                &input.title,
                &input.file_kind,
                &input.section,
                input.body.as_deref().unwrap_or(""),
            );
            if let Some(order_key) = input.order_key {
                manifest.set_order_key(order_key);
            }
            if !input.repo_scope.is_empty() {
                manifest.set_repo_scopes(&input.repo_scope);
            }
            if !input.path_scope.is_empty() {
                manifest.set_path_scopes(&input.path_scope);
            }
            apply_source_location(&mut manifest, &input)?;

            let id = store.create(&manifest, None).map_err(Self::rule_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "id": id,
                "slug": manifest.slug(),
                "title": manifest.title(),
                "file_kind": manifest.file_kind(),
                "section": manifest.section(),
            }))
        })
        .await
    }

    pub(super) async fn rule_get_tool(
        &self,
        input: RuleRefInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let rule = store.get(&input.id).map_err(Self::rule_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "rule": rule_json(&rule),
            }))
        })
        .await
    }

    pub(super) async fn rule_update_tool(
        &self,
        input: UpdateRuleInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            if let Some(body) = &input.body {
                store.update_body(&input.id, body).map_err(Self::rule_err)?;
            }
            let patch = parse_fields(&input.fields)?;
            let rule = store
                .update(&input.id, patch, input.to_state.as_deref())
                .map_err(Self::rule_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "rule": rule_json(&rule),
            }))
        })
        .await
    }

    pub(super) async fn rule_list_tool(
        &self,
        input: ListRulesInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let filter = rule_filter(
                input.state,
                input.file_kind,
                input.section,
                input.repo_scope,
                input.path_scope,
                input.slug,
                input.unresolved_only,
            );
            let rules = store.list(&filter, input.limit).map_err(Self::rule_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "count": rules.len(),
                "items": rules.iter().map(rule_summary_json).collect::<Vec<_>>(),
            }))
        })
        .await
    }

    pub(super) async fn rule_search_tool(
        &self,
        input: SearchRulesInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let filter = rule_filter(
                input.state,
                input.file_kind,
                input.section,
                input.repo_scope,
                input.path_scope,
                input.slug,
                input.unresolved_only,
            );
            let rules = store
                .search(&input.query, &filter, input.limit)
                .map_err(Self::rule_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "query": input.query,
                "count": rules.len(),
                "items": rules.iter().map(rule_summary_json).collect::<Vec<_>>(),
            }))
        })
        .await
    }
}

fn apply_source_location(
    manifest: &mut RuleManifest,
    input: &CreateRuleInput,
) -> Result<(), McpError> {
    match (
        input.source_repo.as_deref(),
        input.source_path.as_deref(),
        input.source_start_line,
        input.source_end_line,
    ) {
        (Some(repo), Some(path), Some(start), Some(end)) => {
            manifest.set_source_location(repo, path, start, end);
            Ok(())
        }
        (None, None, None, None) => Ok(()),
        _ => Err(McpError::invalid_params(
            "source location requires source_repo, source_path, source_start_line, and source_end_line together".to_string(),
            None,
        )),
    }
}

fn rule_filter(
    state: Option<String>,
    file_kind: Option<String>,
    section: Option<String>,
    repo_scope: Option<String>,
    path_scope: Option<String>,
    slug: Option<String>,
    unresolved_only: bool,
) -> RuleFilter {
    RuleFilter {
        state,
        file_kind,
        section,
        repo_scope,
        path_scope,
        slug,
        has_unresolved_feedback: unresolved_only.then_some(true),
    }
}

fn rule_json(rule: &RuleManifest) -> Value {
    json!({
        "id": rule.id,
        "created_at": rule.created_at,
        "fields": &rule.extra,
    })
}

fn rule_summary_json(rule: &RuleManifest) -> Value {
    json!({
        "id": rule.id,
        "slug": rule.slug(),
        "title": rule.title(),
        "state": rule.state(),
        "file_kind": rule.file_kind(),
        "section": rule.section(),
        "repo_scopes": rule.repo_scopes(),
        "path_scopes": rule.path_scopes(),
        "order_key": rule.order_key(),
        "feedback_unresolved_count": rule.feedback_unresolved_count(),
    })
}

fn parse_fields(
    fields: &[String]
) -> Result<BTreeMap<String, Value>, McpError> {
    let mut patch = BTreeMap::new();
    for field in fields {
        let (key, value) = field.split_once('=').ok_or_else(|| {
            McpError::invalid_params(
                format!("invalid field format '{field}', expected key=value"),
                None,
            )
        })?;
        patch.insert(
            key.trim().to_string(),
            Value::String(value.trim().to_string()),
        );
    }
    Ok(patch)
}
