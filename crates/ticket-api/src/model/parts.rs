//! Core part-kind vocabulary for ticket content parts. See spec 24b3d22b.
//!
//! Core kinds are schema-validated and interpreted by projections
//! (a follow-up ticket). Any other kind is accepted and stored as an
//! opaque attachment: preserved, listed, and retrievable, but not
//! interpreted.

/// The schema-validated core part kinds understood by projections.
pub const CORE_PART_KINDS: &[&str] = &[
    "objective",
    "requirements",
    "design",
    "examples",
    "acceptance_criteria",
    "review",
    "validation",
    "notes",
    "amendment",
];

/// The planning-phase kinds frozen when a ticket enters `planned` (spec
/// 24b3d22b, ticket f9e70385). `review`, `validation`, `notes`, `amendment`,
/// and free-form kinds are never frozen — they stay writable in every state
/// so recording progress never requires touching the plan.
pub const PLANNING_PART_KINDS: &[&str] = &[
    "objective",
    "requirements",
    "design",
    "examples",
    "acceptance_criteria",
];

/// Returns `true` when `kind` is one of [`PLANNING_PART_KINDS`].
pub fn is_planning_part_kind(kind: &str) -> bool {
    PLANNING_PART_KINDS.contains(&kind)
}

/// Classification of a part's `kind` string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartKindClass {
    /// One of [`CORE_PART_KINDS`], exact case-sensitive match.
    Core(String),
    /// Any other kind string: an opaque, free-form attachment.
    Attachment(String),
}

impl PartKindClass {
    /// The underlying kind string, regardless of classification.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Core(kind) | Self::Attachment(kind) => kind,
        }
    }

    pub fn is_core(&self) -> bool {
        matches!(self, Self::Core(_))
    }
}

/// Returns `true` when `kind` exactly matches one of [`CORE_PART_KINDS`].
pub fn is_core_part_kind(kind: &str) -> bool {
    CORE_PART_KINDS.contains(&kind)
}

/// Classify a part `kind` string as core (schema-validated) or an opaque
/// attachment kind. Never fails: every string is a valid attachment kind
/// unless it exactly matches a core kind.
pub fn classify_part_kind(kind: &str) -> PartKindClass {
    if is_core_part_kind(kind) {
        PartKindClass::Core(kind.to_string())
    } else {
        PartKindClass::Attachment(kind.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_core_kinds_classify_as_core() {
        for &kind in CORE_PART_KINDS {
            assert_eq!(
                classify_part_kind(kind),
                PartKindClass::Core(kind.to_string())
            );
            assert!(is_core_part_kind(kind));
        }
    }

    #[test]
    fn unknown_kind_classifies_as_attachment() {
        let class = classify_part_kind("handoff_package");
        assert_eq!(
            class,
            PartKindClass::Attachment("handoff_package".to_string())
        );
        assert!(!class.is_core());
        assert!(!is_core_part_kind("handoff_package"));
    }

    #[test]
    fn core_kind_match_is_case_sensitive() {
        // Only exact, lowercase core-kind strings are schema-validated;
        // anything else — including case variants — passes through as an
        // opaque attachment rather than erroring.
        let class = classify_part_kind("Objective");
        assert!(!class.is_core());
    }

    #[test]
    fn empty_kind_is_an_attachment_not_an_error() {
        let class = classify_part_kind("");
        assert_eq!(class, PartKindClass::Attachment(String::new()));
    }
}
