use std::path::PathBuf;

use memory_api::model::filesystem::ScanRoot;
use rmcp::{ErrorData as McpError, model::CallToolResult};
use serde_json::json;

use super::{AddRootInput, RuleServer, ScanInput};

impl RuleServer {
    pub(super) async fn rule_scan_tool(
        &self,
        input: ScanInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let report = store.scan(input.force).map_err(Self::rule_err)?;
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

    pub(super) async fn rule_add_root_tool(
        &self,
        input: AddRootInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let path = PathBuf::from(&input.path);
            let label = input.label.unwrap_or_else(|| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("rules")
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