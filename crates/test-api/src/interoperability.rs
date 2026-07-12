/// A cross-cutting contract for artifacts that need to interoperate within the
/// workspace.
///
/// Ensures shared baseline standards for self-description, identity,
/// lineage, and lineage-correlation.
pub trait InteroperableArtifact {
    /// The specific class or type of the artifact, e.g. "validation-execution".
    fn artifact_class(&self) -> &'static str;

    /// Retrieve any genuinely dynamic interoperability gaps.
    ///
    /// By default, returns an empty list, assuming the type's structure
    /// guarantees contract compliance. Genuinely dynamic validation checks
    /// (e.g. blank field values at runtime) should be listed here.
    fn interoperability_gaps(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

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
