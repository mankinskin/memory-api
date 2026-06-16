pub mod code_ref;
pub mod default_schema;
pub mod error;
pub mod manifest;
pub mod slug;
pub mod store;
pub mod store_index;

pub use memory_api::generated_markdown::GeneratedMarkdownSnippet;

pub use code_ref::{
    CodeRef,
    SymbolKind,
};
pub use default_schema::{
    spec_schema_registry,
    specification_schema,
};
pub use manifest::{
    AcceptanceCriterion,
    EvidenceRequirement,
    ExpectedProperty,
    FulfillmentStatus,
    FulfillmentSubjectKind,
    FulfillmentSummary,
    SpecContractMode,
    SpecHealthFinding,
    SpecHealthReport,
    SpecManifest,
};
pub use slug::{
    SlugIndex,
    validate_slug,
};
pub use store::{
    GENERATED_BODY_FILE_COMMENT,
    GENERATED_SPEC_FILE_COMMENT,
    SpecStore,
    render_generated_body,
    render_generated_document,
};
pub use store_index::{
    SPEC_INDEX_AGENT_HOOK_PATH,
    SpecCatalogArtifacts,
    SpecCatalogSource,
    generate_spec_catalog,
};
