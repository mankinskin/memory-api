pub mod default_schema;
pub mod error;
pub mod import;
pub mod manifest;
pub mod render;
pub mod store;
pub mod targets;
pub mod workspace;

pub use default_schema::{
    RULE_ENTRY_SCHEMA_TOML,
    rule_entry_schema,
    rule_schema_registry,
};
pub use import::{
    ImportedRuleBlock,
    MarkdownImportOptions,
    import_markdown_blocks,
};
pub use manifest::{
    RuleManifest,
    RuleState,
};
pub use render::{
    GENERATED_FILE_COMMENT,
    render_markdown_file,
};
pub use store::{
    RuleFilter,
    RuleStore,
};
pub use targets::{
    ExplainedRuleMatch,
    ExplainedTarget,
    ExplainedTargetNode,
    RenderTarget,
    RenderTargetConfig,
    RenderTargetFilter,
    RenderTargetNode,
    TargetConfigError,
    collect_target_rules,
    explain_target,
    load_render_target_config,
    render_target_by_name,
    resolve_render_target_output,
};
pub use workspace::{
    discover_workspace_scan_roots,
    workspace_root_for_index_root,
};
