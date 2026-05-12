use rmcp::schemars::{
    self,
    JsonSchema,
};
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuleRefInput {
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateRuleInput {
    pub id: String,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub to_state: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordFeedbackInput {
    pub id: String,
    pub rating: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub note_kind: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub agent_or_user_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImportRuleFileInput {
    pub path: String,
    pub file_kind: String,
    pub repo_scope: Vec<String>,
    pub slug_prefix: String,
    #[serde(default)]
    pub default_section: Option<String>,
    #[serde(default)]
    pub path_scope: Vec<String>,
    #[serde(default)]
    pub source_repo: Option<String>,
    #[serde(default)]
    pub target_root: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateRuleInput {
    pub title: String,
    pub slug: String,
    pub file_kind: String,
    pub section: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub repo_scope: Vec<String>,
    #[serde(default)]
    pub path_scope: Vec<String>,
    #[serde(default)]
    pub order_key: Option<i64>,
    #[serde(default)]
    pub source_repo: Option<String>,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub source_start_line: Option<i64>,
    #[serde(default)]
    pub source_end_line: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListRulesInput {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub file_kind: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub repo_scope: Option<String>,
    #[serde(default)]
    pub path_scope: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub low_rated_only: bool,
    #[serde(default)]
    pub unresolved_only: bool,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchRulesInput {
    pub query: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub file_kind: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub repo_scope: Option<String>,
    #[serde(default)]
    pub path_scope: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub low_rated_only: bool,
    #[serde(default)]
    pub unresolved_only: bool,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GenerateRuleFileInput {
    pub file_kind: String,
    pub repo_scope: String,
    #[serde(default)]
    pub path_scope: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub check: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GenerateRuleTargetInput {
    pub config_path: String,
    pub target: String,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub check: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExplainRuleTargetInput {
    pub config_path: String,
    pub target: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanInput {
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddRootInput {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
}

fn default_search_limit() -> usize {
    20
}
