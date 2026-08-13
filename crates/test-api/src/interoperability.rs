pub use memory_kernel::InteroperableArtifact;

/// A contract for artifacts that are uniquely identifiable.
pub trait IdentifiableArtifact {
    type Id: AsRef<str> + PartialEq + ?Sized;

    /// Return the unique identifier for this artifact.
    fn id(&self) -> &Self::Id;
}

/// A contract for artifacts that are traceable (associated with dynamic run provenance
/// and spec/ticket links).
pub trait TraceableArtifact: InteroperableArtifact {
    /// Return the optional/required domain of test or execution.
    fn domain(&self) -> Option<&str>;

    /// Return the optional/required operation.
    fn operation(&self) -> Option<&str>;

    /// Return the optional/required execution run identifier.
    fn run_id(&self) -> Option<&str>;

    /// Return true if this artifact specifies explicit traceability links
    /// (e.g. spec_ids, ticket_ids).
    fn has_traceability_links(&self) -> bool;
}
