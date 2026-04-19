use std::collections::HashMap;
use uuid::Uuid;
use crate::error::SpecError;

/// Validate a slug string.
///
/// Rules:
/// - Must not be empty
/// - Segments separated by `/`
/// - Each segment: lowercase `[a-z0-9]` and hyphens `-`
/// - No empty segments (no `//`, no leading/trailing `/`)
/// - No uppercase letters
/// - No special chars other than `-` and `/`
pub fn validate_slug(slug: &str) -> Result<(), SpecError> {
    if slug.is_empty() {
        return Err(SpecError::InvalidSlug("slug cannot be empty".into()));
    }
    if slug.starts_with('/') || slug.ends_with('/') {
        return Err(SpecError::InvalidSlug("slug cannot start or end with '/'".into()));
    }
    for segment in slug.split('/') {
        if segment.is_empty() {
            return Err(SpecError::InvalidSlug("slug contains empty segment".into()));
        }
        if !segment.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(SpecError::InvalidSlug(
                format!("segment '{}' contains invalid characters (only a-z, 0-9, - allowed)", segment)
            ));
        }
        if segment.starts_with('-') || segment.ends_with('-') {
            return Err(SpecError::InvalidSlug(
                format!("segment '{}' cannot start or end with '-'", segment)
            ));
        }
    }
    Ok(())
}

/// In-memory slug → UUID index with uniqueness enforcement.
#[derive(Debug, Default)]
pub struct SlugIndex {
    map: HashMap<String, Uuid>,
}

impl SlugIndex {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    /// Rebuild the index from a list of (slug, id) pairs.
    /// Returns an error if duplicates are found.
    pub fn rebuild(entries: impl IntoIterator<Item = (String, Uuid)>) -> Result<Self, SpecError> {
        let mut index = Self::new();
        for (slug, id) in entries {
            index.insert(slug, id)?;
        }
        Ok(index)
    }

    /// Insert a slug → UUID mapping. Returns error if slug already exists with a different UUID.
    pub fn insert(&mut self, slug: String, id: Uuid) -> Result<(), SpecError> {
        validate_slug(&slug)?;
        if let Some(existing) = self.map.get(&slug) {
            if *existing != id {
                return Err(SpecError::DuplicateSlug(slug));
            }
        }
        self.map.insert(slug, id);
        Ok(())
    }

    /// Resolve a slug to its UUID.
    pub fn resolve(&self, slug: &str) -> Option<Uuid> {
        self.map.get(slug).copied()
    }

    /// Remove a slug from the index.
    pub fn remove(&mut self, slug: &str) -> Option<Uuid> {
        self.map.remove(slug)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_slug ──

    #[test]
    fn test_valid_slugs() {
        assert!(validate_slug("ticket-api").is_ok());
        assert!(validate_slug("ticket-api/storage").is_ok());
        assert!(validate_slug("ticket-api/storage/store").is_ok());
        assert!(validate_slug("a").is_ok());
        assert!(validate_slug("abc-123").is_ok());
        assert!(validate_slug("a/b/c/d").is_ok());
    }

    #[test]
    fn test_invalid_empty() {
        assert!(validate_slug("").is_err());
    }

    #[test]
    fn test_invalid_leading_trailing_slash() {
        assert!(validate_slug("/ticket-api").is_err());
        assert!(validate_slug("ticket-api/").is_err());
        assert!(validate_slug("/").is_err());
    }

    #[test]
    fn test_invalid_double_slash() {
        assert!(validate_slug("ticket-api//store").is_err());
    }

    #[test]
    fn test_invalid_uppercase() {
        assert!(validate_slug("Ticket-Api").is_err());
        assert!(validate_slug("TICKET").is_err());
    }

    #[test]
    fn test_invalid_special_chars() {
        assert!(validate_slug("ticket_api").is_err()); // underscore
        assert!(validate_slug("ticket.api").is_err()); // dot
        assert!(validate_slug("ticket api").is_err()); // space
        assert!(validate_slug("ticket@api").is_err()); // at
    }

    #[test]
    fn test_invalid_leading_trailing_hyphen() {
        assert!(validate_slug("-ticket").is_err());
        assert!(validate_slug("ticket-").is_err());
        assert!(validate_slug("a/-b").is_err());
    }

    // ── SlugIndex ──

    #[test]
    fn test_slug_index_insert_resolve() {
        let mut idx = SlugIndex::new();
        let id = Uuid::new_v4();
        idx.insert("ticket-api/store".into(), id).unwrap();
        assert_eq!(idx.resolve("ticket-api/store"), Some(id));
        assert_eq!(idx.resolve("nonexistent"), None);
    }

    #[test]
    fn test_slug_index_duplicate_error() {
        let mut idx = SlugIndex::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        idx.insert("ticket-api/store".into(), id1).unwrap();
        assert!(idx.insert("ticket-api/store".into(), id2).is_err());
    }

    #[test]
    fn test_slug_index_same_id_ok() {
        let mut idx = SlugIndex::new();
        let id = Uuid::new_v4();
        idx.insert("ticket-api/store".into(), id).unwrap();
        // Re-inserting same slug with same ID is OK (idempotent)
        assert!(idx.insert("ticket-api/store".into(), id).is_ok());
    }

    #[test]
    fn test_slug_index_remove() {
        let mut idx = SlugIndex::new();
        let id = Uuid::new_v4();
        idx.insert("ticket-api/store".into(), id).unwrap();
        assert_eq!(idx.remove("ticket-api/store"), Some(id));
        assert_eq!(idx.resolve("ticket-api/store"), None);
    }

    #[test]
    fn test_slug_index_rebuild() {
        let entries = vec![
            ("a/b".into(), Uuid::new_v4()),
            ("c/d".into(), Uuid::new_v4()),
        ];
        let idx = SlugIndex::rebuild(entries).unwrap();
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn test_slug_index_rebuild_duplicate_error() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let entries = vec![
            ("same-slug".into(), id1),
            ("same-slug".into(), id2),
        ];
        assert!(SlugIndex::rebuild(entries).is_err());
    }
}
