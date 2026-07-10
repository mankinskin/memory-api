pub mod discovery;
pub mod error;
pub mod generated_markdown;
pub mod index_generator;
pub mod model;
pub mod runtime;
pub mod storage;
pub mod workspace;
pub mod workspace_policy;

#[cfg(feature = "testing")]
pub mod testing;

// Re-export board types at the crate root for convenient access.
pub use storage::{
    BoardCleanPreview,
    BoardCleanResult,
    BoardConfig,
    BoardEntry,
    BoardEntryStatus,
    BoardError,
    BoardReconcileResult,
    BoardSnapshot,
    EntityStore,
    ReconcileAction,
};

// Re-export index entry schema types at the crate root for convenient access.
pub use discovery::{
    DiscoveredStore,
    IntegrationStatus,
    ReconcileSummary,
    STORE_MARKERS,
    StoreReport,
    discover_stores,
    reconcile_stores,
    summarize,
};
pub use model::{
    index_entry::{
        ContentKind,
        IndexEntry,
        IndexRef,
        IndexRelations,
        RelationKind,
    },
    index_sidecar::{
        IndexSidecar,
        SidecarError,
        SidecarValidationIssue,
        read_sidecar,
        write_sidecar,
    },
    urn::{
        URN_SCHEME,
        Urn,
        UrnError,
        UrnResolver,
    },
};
