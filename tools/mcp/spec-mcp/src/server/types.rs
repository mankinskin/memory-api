use rmcp::schemars::{
    self,
    JsonSchema,
};
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SpecRefInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateSpecInput {
    /// Spec title.
    pub title: String,
    /// Hierarchical slug (e.g. "ticket-api/storage/store").
    pub slug: String,
    /// Component this spec belongs to.
    pub component: String,
    /// Parent spec ID or slug.
    #[serde(default)]
    pub parent: Option<String>,
    /// Scope (e.g. "public", "internal").
    #[serde(default)]
    pub scope: Option<String>,
    /// Body content (markdown).
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSpecInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Include body and sections in output.
    #[serde(default)]
    pub full: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateSpecInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Field patches as key=value pairs (e.g. ["title=New Title", "state=active"]).
    #[serde(default)]
    pub fields: Vec<String>,
    /// Optional state to transition to.
    #[serde(default)]
    pub to_state: Option<String>,
    /// Optional body content to replace.
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSpecsInput {
    /// Filter by field=value predicates.
    #[serde(default)]
    pub where_clauses: Vec<String>,
    /// Maximum results.
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSpecsInput {
    /// Full-text search query.
    pub query: String,
    /// Maximum results.
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    20
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TreeInput {
    /// Root spec ID or slug (omit for all roots).
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HealthInput {
    /// Spec UUID, prefix, or slug (omit with all=true for all specs).
    #[serde(default)]
    pub id: Option<String>,
    /// Check all specs.
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RefsValidateInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Workspace root for resolving file paths.
    #[serde(default = "default_workspace_root")]
    pub workspace_root: String,
}

fn default_workspace_root() -> String {
    ".".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SectionAddInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Section name.
    pub name: String,
    /// Section content (markdown).
    pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SectionRefInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Section name.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanInput {
    /// Force full reindex.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddRootInput {
    /// Directory path to register as a scan root.
    pub path: String,
    /// Optional label for this root.
    #[serde(default)]
    pub label: Option<String>,
}
