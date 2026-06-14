use std::{
    collections::BTreeMap,
    fs::{
        self,
        File,
        OpenOptions,
    },
    io::{
        BufRead,
        BufReader,
        Write,
    },
    path::{
        Path,
        PathBuf,
    },
};

use chrono::Utc;
use fs4::fs_std::FileExt;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    error::StorageError,
    model::{
        entity::EntityManifest,
        filesystem::{
            EntityFolderConfig,
            ParseDiagnostic,
            parse_entity_manifest_toml,
        },
    },
};

/// A single immutable revision snapshot stored in `history.ndjson`.
///
/// Revisions are append-only; `revert` creates a new revision with old state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRevision {
    /// 1-based sequential revision number.
    pub rev: u64,
    /// ISO-8601 UTC timestamp of when this revision was written.
    pub ts: String,
    /// Complete snapshot of the manifest `extra` fields at this revision.
    pub fields: BTreeMap<String, Value>,
    /// Identity of the user or agent who made this change (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

pub struct EntityScanEntry {
    pub id: Uuid,
    pub path: PathBuf,
    pub manifest: EntityManifest,
}

/// Generic filesystem operations for entity folders.
///
/// Each entity lives in a folder named by its UUID:
///
/// ```text
/// <scan_root>/<uuid>/
///   <manifest_file>     ← manifest (TOML), e.g. ticket.toml or spec.toml
///   <lock_file>         ← advisory lock file during writes
///   assets/             ← optional attachments
///   history.ndjson      ← append-only revision log
/// ```
///
/// Configure the manifest and lock filenames via [`EntityFolderConfig`].
pub struct EntityFs {
    pub config: EntityFolderConfig,
}

impl EntityFs {
    pub const fn new(
        manifest_file: &'static str,
        lock_file: &'static str,
    ) -> Self {
        Self {
            config: EntityFolderConfig::new(manifest_file, lock_file),
        }
    }

    pub const fn with_config(config: EntityFolderConfig) -> Self {
        Self { config }
    }

    /// Create a new entity folder under `target_root`.
    ///
    /// Protocol:
    /// 1. Write manifest to a temp folder `<uuid>.tmp/`
    /// 2. Rename temp → final `<uuid>/` (atomic on POSIX; best-effort on Windows)
    ///
    /// Returns the absolute path to the created entity folder.
    pub fn create(
        &self,
        manifest: &EntityManifest,
        target_root: &Path,
        body: Option<&str>,
    ) -> Result<PathBuf, StorageError> {
        let uuid_str = manifest.id.to_string();
        let final_dir = target_root.join(&uuid_str);
        let temp_dir = target_root.join(format!("{}.tmp", uuid_str));

        if final_dir.exists() {
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "entity folder already exists: {}",
                    final_dir.display()
                ),
            )));
        }

        fs::create_dir_all(&temp_dir)?;
        self.write_manifest(&temp_dir, manifest)?;
        if let Some(text) = body {
            fs::write(temp_dir.join(self.config.body_file), text)?;
        }

        fs::rename(&temp_dir, &final_dir).map_err(|e| {
            let _ = fs::remove_dir_all(&temp_dir);
            StorageError::Io(e)
        })?;

        Ok(final_dir)
    }

    /// Read and parse the manifest from an existing entity folder.
    pub fn read(
        &self,
        entity_path: &Path,
    ) -> Result<EntityManifest, StorageError> {
        let manifest_path = entity_path.join(self.config.manifest_file);
        let content = fs::read_to_string(&manifest_path)?;
        parse_entity_manifest_toml(manifest_path.clone(), &content).map_err(
            |d| StorageError::ParseError {
                path: d.path,
                reason: d.reason,
            },
        )
    }

    /// Apply a field patch to the manifest on disk.
    ///
    /// Protocol:
    /// 1. Acquire lock file (exclusive)
    /// 2. Read + merge patch
    /// 3. Write updated manifest
    /// 4. Release lock
    pub fn update(
        &self,
        entity_path: &Path,
        patch: &BTreeMap<String, Value>,
        new_state: Option<&str>,
    ) -> Result<EntityManifest, StorageError> {
        let lock_path = entity_path.join(self.config.lock_file);
        let lock_file = acquire_lock(&lock_path)?;

        let result = (|| -> Result<EntityManifest, StorageError> {
            let mut manifest = self.read(entity_path)?;
            for (k, v) in patch {
                manifest.extra.insert(k.clone(), v.clone());
            }
            if let Some(state) = new_state {
                manifest.extra.insert(
                    "state".to_string(),
                    Value::String(state.to_string()),
                );
            }
            self.write_manifest(entity_path, &manifest)?;
            Ok(manifest)
        })();

        release_lock(&lock_file, &lock_path);
        result
    }

    /// Physically delete an entity folder from disk.
    pub fn delete(
        &self,
        entity_path: &Path,
    ) -> Result<(), StorageError> {
        std::fs::remove_dir_all(entity_path).map_err(StorageError::Io)
    }

    /// Walk `scan_root` and locate all valid entity folders.
    ///
    /// Returns `(valid_paths, parse_diagnostics)`.
    pub fn scan_root(
        &self,
        scan_root: &Path,
    ) -> Result<(Vec<EntityScanEntry>, Vec<ParseDiagnostic>), StorageError>
    {
        let mut valid = Vec::new();
        let mut diags = Vec::new();

        let Some(read_dir) = read_scan_root(scan_root)? else {
            return Ok((valid, diags));
        };

        for entry in read_dir.flatten() {
            let Some((path, id)) = scan_candidate(entry.path()) else {
                continue;
            };

            match self.load_scan_entry(path, id) {
                Ok(Some(entry)) => valid.push(entry),
                Ok(None) => {},
                Err(diag) => diags.push(diag),
            }
        }

        Ok((valid, diags))
    }

    fn load_scan_entry(
        &self,
        path: PathBuf,
        id: Uuid,
    ) -> Result<Option<EntityScanEntry>, ParseDiagnostic> {
        let manifest_path = path.join(self.config.manifest_file);
        if !manifest_path.exists() {
            return Err(ParseDiagnostic {
                path: manifest_path,
                reason: format!("missing {}", self.config.manifest_file),
            });
        }

        match self.read(&path) {
            Ok(manifest) => Ok(Some(EntityScanEntry { id, path, manifest })),
            Err(StorageError::ParseError { path, reason }) =>
                Err(ParseDiagnostic { path, reason }),
            Err(error) => Err(ParseDiagnostic {
                path: manifest_path,
                reason: error.to_string(),
            }),
        }
    }

    // ── history ───────────────────────────────────────────────────────────────

    /// Read all history revisions for an entity (oldest first).
    pub fn read_history(
        &self,
        entity_path: &Path,
    ) -> Result<Vec<HistoryRevision>, StorageError> {
        let path = entity_path.join(self.config.history_file);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let mut revisions = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let rev: HistoryRevision = serde_json::from_str(trimmed)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            revisions.push(rev);
        }
        Ok(revisions)
    }

    /// Append one revision snapshot to `history.ndjson`.
    pub fn append_history(
        &self,
        entity_path: &Path,
        fields: BTreeMap<String, Value>,
        author: Option<String>,
    ) -> Result<u64, StorageError> {
        let path = entity_path.join(self.config.history_file);
        let existing_count = self.read_history(entity_path)?.len() as u64;
        let rev = existing_count + 1;
        let entry = HistoryRevision {
            rev,
            ts: Utc::now().to_rfc3339(),
            fields,
            author,
        };
        let line = serde_json::to_string(&entry)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        let mut file =
            OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{}", line)?;
        Ok(rev)
    }

    /// Ensure the assets subdirectory exists inside `entity_path`.
    pub fn ensure_assets_dir(
        &self,
        entity_path: &Path,
    ) -> Result<(), StorageError> {
        let assets = entity_path.join(self.config.assets_dir);
        if !assets.exists() {
            fs::create_dir_all(&assets)?;
        }
        Ok(())
    }

    /// Reformat an existing entity's manifest to canonical field ordering.
    pub fn reformat(
        &self,
        entity_path: &Path,
    ) -> Result<(), StorageError> {
        let lock_path = entity_path.join(self.config.lock_file);
        let lock_file = acquire_lock(&lock_path)?;
        let result = (|| -> Result<(), StorageError> {
            let manifest = self.read(entity_path)?;
            self.write_manifest(entity_path, &manifest)?;
            Ok(())
        })();
        release_lock(&lock_file, &lock_path);
        result
    }

    /// Write or overwrite the configured body markdown file for an entity.
    pub fn write_description(
        &self,
        entity_path: &Path,
        text: &str,
    ) -> Result<(), StorageError> {
        let lock_path = entity_path.join(self.config.lock_file);
        let lock_file = acquire_lock(&lock_path)?;
        let result = fs::write(entity_path.join(self.config.body_file), text)
            .map_err(StorageError::Io);
        release_lock(&lock_file, &lock_path);
        result
    }

    /// Read text content of the configured body markdown file.
    ///
    /// When a domain has migrated away from `description.md`, this also falls
    /// back to the legacy filename so older folders remain readable.
    pub fn read_description(
        &self,
        entity_path: &Path,
    ) -> Option<String> {
        let configured = entity_path.join(self.config.body_file);
        fs::read_to_string(&configured).ok().or_else(|| {
            (self.config.body_file != "description.md")
                .then(|| entity_path.join("description.md"))
                .and_then(|legacy| fs::read_to_string(&legacy).ok())
        })
    }

    // ── internal ──────────────────────────────────────────────────────────────

    fn write_manifest(
        &self,
        dir: &Path,
        manifest: &EntityManifest,
    ) -> Result<(), StorageError> {
        let toml_str =
            crate::model::manifest_format::format_manifest_toml(manifest);
        let path = dir.join(self.config.manifest_file);
        fs::write(&path, toml_str)?;
        Ok(())
    }
}

// ── lock helpers ──────────────────────────────────────────────────────────────

fn acquire_lock(lock_path: &Path) -> Result<File, StorageError> {
    let file = File::create(lock_path)?;
    file.lock_exclusive().map_err(StorageError::Io)?;
    Ok(file)
}

fn release_lock(
    file: &File,
    lock_path: &Path,
) {
    let _ = file.unlock();
    let _ = fs::remove_file(lock_path);
}

fn read_scan_root(
    scan_root: &Path
) -> Result<Option<fs::ReadDir>, StorageError> {
    match fs::read_dir(scan_root) {
        Ok(read_dir) => Ok(Some(read_dir)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(StorageError::Io(error)),
    }
}

fn scan_candidate(path: PathBuf) -> Option<(PathBuf, Uuid)> {
    if !path.is_dir() {
        return None;
    }

    let name = path.file_name().and_then(|value| value.to_str())?;
    if name.ends_with(".tmp") {
        return None;
    }

    let id = name.parse().ok()?;
    Some((path, id))
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::HistoryRevision;
    use serde_json::Value;
    use std::collections::BTreeMap;

    #[test]
    fn history_revision_backward_compat_no_author() {
        let json = r#"{"rev":1,"ts":"2025-01-01T00:00:00Z","fields":{"state":"new","title":"Old entry"}}"#;
        let rev: HistoryRevision = serde_json::from_str(json)
            .expect("should deserialize legacy revision without author field");
        assert_eq!(rev.rev, 1);
        assert_eq!(
            rev.author, None,
            "author should be None for legacy entries"
        );
    }

    #[test]
    fn history_revision_with_author() {
        let json = r#"{"rev":2,"ts":"2025-01-02T00:00:00Z","fields":{},"author":"alice"}"#;
        let rev: HistoryRevision = serde_json::from_str(json)
            .expect("should deserialize revision with author");
        assert_eq!(rev.author, Some("alice".to_string()));
    }

    #[test]
    fn history_revision_none_author_is_skipped_in_serialization() {
        let rev = HistoryRevision {
            rev: 1,
            ts: "2025-01-01T00:00:00Z".to_string(),
            fields: BTreeMap::new(),
            author: None,
        };
        let json = serde_json::to_string(&rev).expect("serialize");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert!(
            v.get("author").is_none(),
            "author key should be absent when None"
        );
    }
}
