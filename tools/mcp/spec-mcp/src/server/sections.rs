use std::path::PathBuf;

use memory_api::model::filesystem::ScanRoot;
use rmcp::{ErrorData as McpError, model::CallToolResult};
use serde_json::json;

use super::{
    AddRootInput, ScanInput, SectionAddInput, SectionRefInput, SpecRefInput, SpecServer,
};

impl SpecServer {
    pub(super) async fn spec_section_add_tool(
        &self,
        input: SectionAddInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            store
                .add_section(&input.id, &input.name, &input.content)
                .map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "spec": input.id,
                "section": input.name,
            }))
        })
        .await
    }

    pub(super) async fn spec_section_list_tool(
        &self,
        input: SpecRefInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let sections = store.list_sections(&input.id).map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "spec": input.id,
                "count": sections.len(),
                "sections": sections,
            }))
        })
        .await
    }

    pub(super) async fn spec_section_get_tool(
        &self,
        input: SectionRefInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let uuid = store.resolve_id(&input.id).map_err(Self::spec_err)?;
            let indexed = store
                .entity_store()
                .get_indexed(&uuid)
                .map_err(Self::storage_err)?
                .ok_or_else(|| McpError::invalid_params("spec not found", None))?;
            let file_name = if input.name.ends_with(".md") {
                input.name.clone()
            } else {
                format!("{}.md", input.name)
            };
            let path = indexed.path.join("sections").join(&file_name);
            let content = std::fs::read_to_string(&path)
                .map_err(|error| McpError::invalid_params(format!("section not found: {error}"), None))?;
            Self::json_result(&json!({
                "status": "ok",
                "spec": input.id,
                "section": input.name,
                "content": content,
            }))
        })
        .await
    }

    pub(super) async fn spec_section_delete_tool(
        &self,
        input: SectionRefInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            store
                .delete_section(&input.id, &input.name)
                .map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "spec": input.id,
                "section": input.name,
            }))
        })
        .await
    }

    pub(super) async fn spec_scan_tool(
        &self,
        input: ScanInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let report = store.scan(input.force).map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "force": input.force,
                "integrated": report.integrated,
                "pruned": report.pruned,
                "diagnostics_count": report.diagnostics.len(),
            }))
        })
        .await
    }

    pub(super) async fn spec_add_root_tool(
        &self,
        input: AddRootInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
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
            Self::json_result(&json!({
                "status": "ok",
                "path": path,
                "label": label,
            }))
        })
        .await
    }
}