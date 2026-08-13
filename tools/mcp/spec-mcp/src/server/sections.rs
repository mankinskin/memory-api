use std::path::PathBuf;

use memory_kernel::model::filesystem::ScanRoot;
use rmcp::{
    ErrorData as McpError,
    model::CallToolResult,
};
use serde_json::json;

use super::{
    AddRootInput,
    ScanInput,
    SectionAddInput,
    SectionRefInput,
    SpecRefInput,
    SpecServer,
};

impl SpecServer {
    pub(super) async fn spec_section_add_tool(
        &self,
        input: SectionAddInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            store
                .add_section(&input.id, &input.name, &input.content)
                .map_err(Self::spec_err)?;
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "spec": input.id,
                    "section": input.name,
                }),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }

    pub(super) async fn spec_section_list_tool(
        &self,
        input: SpecRefInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            let sections =
                store.list_sections(&input.id).map_err(Self::spec_err)?;
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "spec": input.id,
                    "count": sections.len(),
                    "sections": sections,
                }),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }

    pub(super) async fn spec_section_get_tool(
        &self,
        input: SectionRefInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            let uuid = store.resolve_id(&input.id).map_err(Self::spec_err)?;
            let indexed = store
                .entity_store()
                .get_indexed(&uuid)
                .map_err(Self::storage_err)?
                .ok_or_else(|| {
                    McpError::invalid_params("spec not found", None)
                })?;
            let file_name = if input.name.ends_with(".md") {
                input.name.clone()
            } else {
                format!("{}.md", input.name)
            };
            let path = indexed.path.join("sections").join(&file_name);
            let content = std::fs::read_to_string(&path).map_err(|error| {
                McpError::invalid_params(
                    format!("section not found: {error}"),
                    None,
                )
            })?;
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "spec": input.id,
                    "section": input.name,
                    "content": content,
                }),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }

    pub(super) async fn spec_section_delete_tool(
        &self,
        input: SectionRefInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            store
                .delete_section(&input.id, &input.name)
                .map_err(Self::spec_err)?;
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "spec": input.id,
                    "section": input.name,
                }),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }

    pub(super) async fn spec_scan_tool(
        &self,
        input: ScanInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            let report = store.scan(input.force).map_err(Self::spec_err)?;
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "force": input.force,
                    "integrated": report.integrated,
                    "pruned": report.pruned,
                    "diagnostics_count": report.diagnostics.len(),
                }),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }

    pub(super) async fn spec_add_root_tool(
        &self,
        input: AddRootInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        self.with_store(workspace.as_deref(), |store, index_root| {
            let path = PathBuf::from(&input.path);
            let label = input.label.unwrap_or_else(|| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("specs")
                    .to_string()
            });
            store
                .entity_store()
                .add_scan_root(ScanRoot {
                    path: path.clone(),
                    label: label.clone(),
                })
                .map_err(Self::storage_err)?;
            Self::json_result_with_scope(
                json!({
                    "status": "ok",
                    "path": path,
                    "label": label,
                }),
                index_root,
                workspace.as_deref(),
            )
        })
        .await
    }
}
