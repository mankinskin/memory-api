//! Typed ref-kind vocabulary and URN/path validation for ticket `[[refs]]`
//! entries. See spec 24b3d22b, ticket 9d69e93d.
//!
//! Unlike core part kinds, the ref-kind vocabulary is closed: an unknown
//! kind is always rejected at write time (no "opaque attachment" escape
//! hatch), because a ref's `urn` shape is validated per-kind and there is
//! no way to validate the shape of an unknown kind. Reading a manifest with
//! a foreign kind already present never fails — see
//! `memory_kernel::model::entity::EntityManifest::refs`.

use memory_kernel::model::{
    index_entry::ContentKind,
    urn::Urn,
};

use crate::error::{
    SchemaValidationError,
    StorageError,
};

/// The closed set of ref kinds accepted at write time.
pub const REF_KINDS: &[&str] =
    &["spec", "test_execution", "log", "rule", "file", "commit"];

/// Returns `true` when `kind` is one of [`REF_KINDS`].
pub fn is_known_ref_kind(kind: &str) -> bool {
    REF_KINDS.contains(&kind)
}

fn invalid_kind_err(kind: &str) -> StorageError {
    StorageError::Validation(SchemaValidationError::InvalidRefKind {
        kind: kind.to_string(),
        valid_kinds: REF_KINDS.iter().map(|k| k.to_string()).collect(),
    })
}

fn invalid_urn_err(
    kind: &str,
    urn: &str,
) -> StorageError {
    StorageError::Validation(SchemaValidationError::InvalidRefUrn {
        kind: kind.to_string(),
        urn: urn.to_string(),
    })
}

/// Validate `kind` against the closed [`REF_KINDS`] vocabulary.
pub fn validate_ref_kind(kind: &str) -> Result<(), StorageError> {
    if is_known_ref_kind(kind) {
        Ok(())
    } else {
        Err(invalid_kind_err(kind))
    }
}

/// Validate that `urn` has the shape expected for `kind`. `kind` must
/// already be a member of [`REF_KINDS`] (call [`validate_ref_kind`] first).
pub fn validate_ref_urn(
    kind: &str,
    urn: &str,
) -> Result<(), StorageError> {
    match kind {
        "spec" => {
            let parsed = Urn::parse(urn).map_err(|_| invalid_urn_err(kind, urn))?;
            if parsed.store != ContentKind::Spec {
                return Err(invalid_urn_err(kind, urn));
            }
        },
        "rule" => {
            let parsed = Urn::parse(urn).map_err(|_| invalid_urn_err(kind, urn))?;
            if parsed.store != ContentKind::Rule {
                return Err(invalid_urn_err(kind, urn));
            }
        },
        "test_execution" => {
            validate_ce_shape(urn, "test-execution")
                .map_err(|_| invalid_urn_err(kind, urn))?;
        },
        "log" => {
            validate_ce_shape(urn, "log")
                .map_err(|_| invalid_urn_err(kind, urn))?;
        },
        "file" => {
            validate_repo_relative_path(urn)
                .map_err(|_| invalid_urn_err(kind, urn))?;
        },
        "commit" => {
            validate_commit_sha(urn).map_err(|_| invalid_urn_err(kind, urn))?;
        },
        _ => return Err(invalid_kind_err(kind)),
    }
    Ok(())
}

/// Validate and construct a ref entry, applying both kind and URN-shape
/// validation. The single write-time entry point for callers that add a
/// new `[[refs]]` entry.
pub fn validate_new_ref(
    kind: &str,
    urn: &str,
) -> Result<(), StorageError> {
    validate_ref_kind(kind)?;
    validate_ref_urn(kind, urn)?;
    Ok(())
}

/// Loose `ce://<workspace>/<store-slug>/<id>` shape check for kinds whose
/// entity id is not a UUID (test executions and logs use free-form string
/// ids), so this does not reuse the strict [`Urn`] parser.
fn validate_ce_shape(
    urn: &str,
    expected_store_slug: &str,
) -> Result<(), ()> {
    let rest = urn.strip_prefix("ce://").ok_or(())?;
    let segments: Vec<&str> = rest.split('/').collect();
    if segments.len() != 3 {
        return Err(());
    }
    if segments[0].is_empty() || segments[2].is_empty() {
        return Err(());
    }
    if segments[1] != expected_store_slug {
        return Err(());
    }
    Ok(())
}

/// A `file` ref resolves against the repo root: non-empty, forward-slash,
/// no leading slash, no `..` traversal segment.
fn validate_repo_relative_path(path: &str) -> Result<(), ()> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return Err(());
    }
    if path.split('/').any(|segment| segment == "..") {
        return Err(());
    }
    Ok(())
}

/// A `commit` ref is a git SHA (abbreviated or full): 7-40 lowercase hex
/// characters.
fn validate_commit_sha(sha: &str) -> Result<(), ()> {
    if sha.len() < 7 || sha.len() > 40 {
        return Err(());
    }
    if !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_ref_kinds_accepted() {
        for kind in REF_KINDS {
            assert!(validate_ref_kind(kind).is_ok());
        }
    }

    #[test]
    fn unknown_ref_kind_rejected_with_vocabulary_in_error() {
        let err = validate_ref_kind("doc").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("doc"));
        for kind in REF_KINDS {
            assert!(
                message.contains(kind),
                "error message missing valid kind '{kind}': {message}"
            );
        }
    }

    #[test]
    fn spec_urn_shape_validated() {
        let ok = format!("ce://default/spec/{}", uuid::Uuid::new_v4());
        assert!(validate_ref_urn("spec", &ok).is_ok());
        assert!(validate_ref_urn("spec", "not-a-urn").is_err());
        assert!(
            validate_ref_urn("spec", &format!("ce://default/ticket/{}", uuid::Uuid::new_v4()))
                .is_err()
        );
    }

    #[test]
    fn test_execution_urn_allows_non_uuid_id() {
        assert!(validate_ref_urn("test_execution", "ce://default/test-execution/7f2c1a04").is_ok());
        assert!(validate_ref_urn("test_execution", "ce://default/log/7f2c1a04").is_err());
    }

    #[test]
    fn file_ref_rejects_absolute_and_traversal_paths() {
        assert!(validate_ref_urn("file", "memory-api/crates/ticket-api/src/storage/store.rs").is_ok());
        assert!(validate_ref_urn("file", "/etc/passwd").is_err());
        assert!(validate_ref_urn("file", "../secrets.toml").is_err());
        assert!(validate_ref_urn("file", "a\\b.rs").is_err());
    }

    #[test]
    fn commit_ref_requires_hex_sha_shape() {
        assert!(validate_ref_urn("commit", "7f2c1a04").is_ok());
        assert!(validate_ref_urn("commit", "not-hex!!").is_err());
        assert!(validate_ref_urn("commit", "abc").is_err());
    }
}
