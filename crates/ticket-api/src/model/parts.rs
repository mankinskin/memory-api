//! Core part-kind vocabulary for ticket content parts. See spec 24b3d22b.
//!
//! Core kinds are schema-validated and interpreted by projections
//! (a follow-up ticket). Any other kind is accepted and stored as an
//! opaque attachment: preserved, listed, and retrievable, but not
//! interpreted.

use crate::error::{
    SchemaValidationError,
    StorageError,
};

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

/// The named read-projection view profiles (spec 24b3d22b, ticket
/// 4c7b884e). The single source of truth for profile -> part-kind mapping;
/// every transport (store API, CLI, MCP, HTTP) resolves reads through
/// [`ViewProfile`] rather than re-declaring this vocabulary.
pub const VIEW_PROFILE_NAMES: &[&str] = &["summary", "plan", "review", "full"];

/// A named projection bundle selecting which part kinds (and whether typed
/// `[[refs]]`) an aggregated ticket read includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewProfile {
    /// metadata + `objective`.
    Summary,
    /// metadata + `objective` + `requirements` + `design` + `examples` +
    /// `acceptance_criteria` + refs.
    Plan,
    /// metadata + `acceptance_criteria` + `review` + `validation`.
    Review,
    /// Everything: every part present on the ticket (core and free-form)
    /// plus refs.
    Full,
}

impl ViewProfile {
    /// Parse a wire `view` string. Rejects an unrecognized name, naming the
    /// valid vocabulary.
    pub fn parse(name: &str) -> Result<Self, StorageError> {
        match name {
            "summary" => Ok(Self::Summary),
            "plan" => Ok(Self::Plan),
            "review" => Ok(Self::Review),
            "full" => Ok(Self::Full),
            other => Err(StorageError::Other(format!(
                "unknown view profile '{other}': valid profiles are {}",
                VIEW_PROFILE_NAMES.join(", ")
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Plan => "plan",
            Self::Review => "review",
            Self::Full => "full",
        }
    }

    /// Ordered core part kinds this profile includes, or `None` for `full`,
    /// which includes every kind present on the ticket (core and
    /// free-form) rather than a fixed list.
    pub fn kinds(&self) -> Option<&'static [&'static str]> {
        match self {
            Self::Summary => Some(&["objective"]),
            Self::Plan => Some(&[
                "objective",
                "requirements",
                "design",
                "examples",
                "acceptance_criteria",
            ]),
            Self::Review =>
                Some(&["acceptance_criteria", "review", "validation"]),
            Self::Full => None,
        }
    }

    /// Whether this profile's aggregated output includes typed `[[refs]]`.
    pub fn includes_refs(self) -> bool {
        matches!(self, Self::Plan | Self::Full)
    }

    /// Whether this profile renders each frozen part followed immediately
    /// by its amendments inline (oldest first, newest last), rather than a
    /// separate trailing amendments section (spec 24b3d22b AC10).
    pub fn inlines_amendments(self) -> bool {
        matches!(self, Self::Plan | Self::Full)
    }
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

/// Returns `true` when `kind` has the lexical shape of a core kind: a
/// non-empty, ASCII-lowercase-letters/digits/underscores-only token with no
/// leading or trailing underscore. This is a judgment call, not a derived
/// fact — it exists only to decide whether an unrecognized `kind` is a
/// likely core-kind typo (rejected, AC2) versus a deliberately opaque,
/// free-form attachment kind such as `"design.v2"` or
/// `"my-custom-attachment"` (accepted, unchanged behavior).
fn looks_like_core_kind_shape(kind: &str) -> bool {
    !kind.is_empty()
        && !kind.starts_with('_')
        && !kind.ends_with('_')
        && kind
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Levenshtein edit distance between two strings, used to detect a likely
/// typo of a core kind rather than a deliberately different attachment kind.
fn edit_distance(
    a: &str,
    b: &str,
) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] =
                (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// The maximum edit distance from an exact [`CORE_PART_KINDS`] entry at
/// which a core-kind-shaped `kind` is treated as a likely typo (rejected)
/// rather than a distinct attachment kind (accepted). Chosen conservatively
/// small so genuinely different, if similarly shaped, attachment kinds
/// (e.g. `"handoff_package"`) are never mistaken for a core-kind typo.
const CORE_KIND_TYPO_DISTANCE: usize = 2;

/// Classify a part `kind` string as core (schema-validated) or an opaque
/// attachment kind.
///
/// Returns `Err(StorageError::Validation(SchemaValidationError::InvalidCoreKind))`
/// when `kind` has the lexical shape of a core kind ([`looks_like_core_kind_shape`])
/// and is within [`CORE_KIND_TYPO_DISTANCE`] edits of an entry in
/// [`CORE_PART_KINDS`] but does not exactly match any entry — a likely typo
/// (e.g. `"objectve"`, `"acceptance_criterion"`). Any other non-matching
/// kind, including one that merely happens to share the shape (e.g.
/// `"handoff_package"`), is still accepted as an opaque attachment: only
/// strings that look like a near-miss of a specific core kind are rejected.
pub fn classify_part_kind(kind: &str) -> Result<PartKindClass, StorageError> {
    if is_core_part_kind(kind) {
        return Ok(PartKindClass::Core(kind.to_string()));
    }

    if looks_like_core_kind_shape(kind)
        && CORE_PART_KINDS
            .iter()
            .any(|&core| edit_distance(kind, core) <= CORE_KIND_TYPO_DISTANCE)
    {
        return Err(StorageError::Validation(
            SchemaValidationError::InvalidCoreKind {
                kind: kind.to_string(),
                valid_kinds: CORE_PART_KINDS
                    .iter()
                    .map(|&k| k.to_string())
                    .collect(),
            },
        ));
    }

    Ok(PartKindClass::Attachment(kind.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{
        SchemaValidationError,
        StorageError,
    };

    #[test]
    fn all_core_kinds_classify_as_core() {
        for &kind in CORE_PART_KINDS {
            assert_eq!(
                classify_part_kind(kind).unwrap(),
                PartKindClass::Core(kind.to_string())
            );
            assert!(is_core_part_kind(kind));
        }
    }

    #[test]
    fn unknown_kind_classifies_as_attachment() {
        // Core-kind-shaped but far from every core kind: a distinct
        // attachment kind, not a typo, so it must stay accepted.
        let class = classify_part_kind("handoff_package").unwrap();
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
        // opaque attachment rather than erroring. Uppercase also fails the
        // core-kind shape check, so it never reaches typo detection.
        let class = classify_part_kind("Objective").unwrap();
        assert!(!class.is_core());
    }

    #[test]
    fn empty_kind_is_an_attachment_not_an_error() {
        let class = classify_part_kind("").unwrap();
        assert_eq!(class, PartKindClass::Attachment(String::new()));
    }

    #[test]
    fn typo_of_core_kind_is_rejected_with_offending_kind_and_vocabulary() {
        let err = classify_part_kind("objectve").unwrap_err();
        match err {
            StorageError::Validation(
                SchemaValidationError::InvalidCoreKind { kind, valid_kinds },
            ) => {
                assert_eq!(kind, "objectve");
                assert_eq!(
                    valid_kinds,
                    CORE_PART_KINDS
                        .iter()
                        .map(|&k| k.to_string())
                        .collect::<Vec<_>>()
                );
            },
            other => panic!("expected InvalidCoreKind, got {other:?}"),
        }
    }

    #[test]
    fn near_miss_of_multi_word_core_kind_is_rejected() {
        let err = classify_part_kind("acceptance_criterion").unwrap_err();
        assert!(matches!(
            err,
            StorageError::Validation(
                SchemaValidationError::InvalidCoreKind { .. }
            )
        ));
    }

    #[test]
    fn opaque_kinds_with_unusual_characters_are_still_attachments() {
        assert_eq!(
            classify_part_kind("design.v2").unwrap(),
            PartKindClass::Attachment("design.v2".to_string())
        );
        assert_eq!(
            classify_part_kind("my-custom-attachment").unwrap(),
            PartKindClass::Attachment("my-custom-attachment".to_string())
        );
    }
}
