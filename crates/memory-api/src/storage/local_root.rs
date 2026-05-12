use std::{
    fs,
    io::ErrorKind,
    path::Path,
};

use crate::error::StorageError;

const GITIGNORE_HEADER: &str =
    "# Excluded local index artifacts created by memory-api tools.";

pub fn ensure_sqlite_index_root(
    index_root: &Path,
    db_file_name: &str,
    extra_entries: &[&str],
) -> Result<(), StorageError> {
    fs::create_dir_all(index_root).map_err(StorageError::Io)?;

    let mut entries = vec![
        db_file_name.to_string(),
        format!("{db_file_name}-shm"),
        format!("{db_file_name}-wal"),
    ];
    entries.extend(extra_entries.iter().map(|entry| (*entry).to_string()));

    let borrowed: Vec<&str> = entries.iter().map(String::as_str).collect();
    ensure_gitignore_entries(index_root, &borrowed)
}

pub fn ensure_gitignore_entries(
    index_root: &Path,
    entries: &[&str],
) -> Result<(), StorageError> {
    let gitignore_path = index_root.join(".gitignore");
    let existing = match fs::read_to_string(&gitignore_path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(StorageError::Io(error)),
    };

    let missing: Vec<&str> = entries
        .iter()
        .copied()
        .filter(|entry| !existing.lines().any(|line| line.trim() == *entry))
        .collect();

    if missing.is_empty() {
        return Ok(());
    }

    let mut updated = existing;
    if updated.is_empty() {
        updated.push_str(GITIGNORE_HEADER);
        updated.push('\n');
    } else {
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        if !updated.contains(GITIGNORE_HEADER) {
            updated.push('\n');
            updated.push_str(GITIGNORE_HEADER);
            updated.push('\n');
        }
    }

    for entry in missing {
        updated.push_str(entry);
        updated.push('\n');
    }

    fs::write(gitignore_path, updated).map_err(StorageError::Io)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn ensure_sqlite_index_root_creates_gitignore_when_missing() {
        let dir = tempdir().unwrap();
        let root = dir.path().join(".spec");

        ensure_sqlite_index_root(&root, "entities.db", &["search_index/"])
            .unwrap();

        let gitignore =
            fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(
            gitignore.contains(GITIGNORE_HEADER),
            "missing header: {gitignore}"
        );
        assert!(gitignore.contains("entities.db"));
        assert!(gitignore.contains("entities.db-shm"));
        assert!(gitignore.contains("entities.db-wal"));
        assert!(gitignore.contains("search_index/"));
    }

    #[test]
    fn ensure_gitignore_entries_appends_missing_entries_without_duplicates() {
        let dir = tempdir().unwrap();
        let root = dir.path().join(".rule");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(".gitignore"), "# custom\nsearch_index/\n")
            .unwrap();

        ensure_gitignore_entries(&root, &["entities.db", "search_index/"])
            .unwrap();

        let gitignore =
            fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gitignore.contains("# custom"));
        assert!(gitignore.contains("entities.db"));
        assert_eq!(
            gitignore
                .lines()
                .filter(|line| line.trim() == "search_index/")
                .count(),
            1
        );
    }
}