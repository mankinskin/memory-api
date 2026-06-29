pub mod default_schema;
pub mod error;
pub mod feedback;
pub mod import;
pub mod manifest;
pub mod move_domain;
pub mod render;
pub mod store;
pub mod store_index;
pub mod targets;
pub mod workspace;

pub use default_schema::{
    RULE_ENTRY_SCHEMA_TOML,
    rule_entry_schema,
    rule_schema_registry,
};
pub use feedback::{
    FeedbackNoteKind,
    FeedbackRating,
    RuleFeedbackEvent,
    RuleFeedbackInput,
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
    prepare_generated_output,
    render_markdown_file,
};
pub use store::{
    RuleFilter,
    RuleStore,
};
pub use store_index::{
    RULE_CATALOG_AGENT_HOOK_PATH,
    RuleCatalogArtifacts,
    RuleCatalogSource,
    generate_rule_catalog,
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
    render_target_by_selector,
    resolve_render_target_output,
};
pub use workspace::{
    discover_workspace_scan_roots,
    workspace_root_for_index_root,
};
