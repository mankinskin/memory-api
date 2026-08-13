pub mod code_ref;
pub mod default_schema;
pub mod error;
pub mod manifest;
pub mod move_domain;
pub mod slug;
pub mod store;
pub mod store_index;
pub mod ticket_ref;
pub mod verification;
pub mod workspace;

pub use memory_kernel::generated_markdown::GeneratedMarkdownSnippet;

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
    SPEC_INDEX_TREE_DIR,
    SpecCatalogArtifacts,
    SpecCatalogSource,
    generate_spec_catalog,
};
pub use ticket_ref::TicketRef;
pub use verification::{
    SpecVerificationOutcome,
    parse_guards_from_markdown,
    recompute_spec_verified_state,
};
pub use workspace::workspace_recovery_hint;
