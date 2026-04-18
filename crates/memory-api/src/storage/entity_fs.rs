use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::StorageError;
use crate::model::filesystem::{EntityFolderConfig, ParseDiagnostic, parse_entity_manifest_toml};
use crate::model::entity::EntityManifest;

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
    pub const fn new(manifest_file: &'static str, lock_file: &'static str) -> Self {
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
                format!("entity folder already exists: {}", final_dir.display()),
            )));
        }

        fs::create_dir_all(&temp_dir)?;
        self.write_manifest(&temp_dir, manifest)?;
        if let Some(text) = body {
            fs::write(temp_dir.join("description.md"), text)?;
        }

        fs::rename(&temp_dir, &final_dir).map_err(|e| {
            let _ = fs::remove_dir_all(&temp_dir);
            StorageError::Io(e)
        })?;

        Ok(final_dir)
    }

    /// Read and parse the manifest from an existing entity folder.
    pub fn read(&self, entity_path: &Path) -> Result<EntityManifest, StorageError> {
        let manifest_path = entity_path.join(self.config.manifest_file);
        let content = fs::read_to_string(&manifest_path)?;
        parse_entity_manifest_toml(manifest_path.clone(), &content).map_err(|d| {
            StorageError::ParseError {
                path: d.path,
                reason: d.reason,
            }
        })
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
                manifest
                    .extra
                    .insert("state".to_string(), Value::String(state.to_string()));
            }
            self.write_manifest(entity_path, &manifest)?;
            Ok(manifest)
        })();

        release_lock(&lock_file, &lock_path);
        result
    }

    /// Soft-delete an entity by writing a `deleted = true` marker in the manifest.
    pub fn mark_deleted(&self, entity_path: &Path) -> Result<(), StorageError> {
        let lock_path = entity_path.join(self.config.lock_file);
        let lock_file = acquire_lock(&lock_path)?;

        let result = (|| -> Result<(), StorageError> {
            let mut manifest = self.read(entity_path)?;
            manifest
                .extra
                .insert("deleted".to_string(), Value::Bool(true));
            self.write_manifest(entity_path, &manifest)?;
            Ok(())
        })();

        release_lock(&lock_file, &lock_path);
        result
    }

    /// Walk `scan_root` and locate all valid entity folders.
    ///
    /// Returns `(valid_paths, parse_diagnostics)`.
    pub fn scan_root(
        &self,
        scan_root: &Path,
    ) -> Result<(Vec<EntityScanEntry>, Vec<ParseDiagnostic>), StorageError> {
        let mut valid = Vec::new();
        let mut diags = Vec::new();

        let read_dir = match fs::read_dir(scan_root) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((valid, diags)),
            Err(e) => return Err(StorageError::Io(e)),
        };

        let manifest_file = self.config.manifest_file;

        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if name.ends_with(".tmp") || name.ends_with(".deleted") {
                continue;
            }

            let id: Uuid = match name.parse() {
                Ok(u) => u,
                Err(_) => continue,
            };

            let manifest_path = path.join(manifest_file);
            if !manifest_path.exists() {
                diags.push(ParseDiagnostic {
                    path: manifest_path,
                    reason: format!("missing {}", manifest_file),
                });
                continue;
            }

            match self.read(&path) {
                Ok(manifest) => {
                    let is_deleted = manifest
                        .extra
                        .get("deleted")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if !is_deleted {
                        valid.push(EntityScanEntry { id, path, manifest });
                    }
                }
                Err(StorageError::ParseError { path: p, reason }) => {
                    diags.push(ParseDiagnostic { path: p, reason });
                }
                Err(e) => {
                    diags.push(ParseDiagnostic {
                        path: manifest_path,
                        reason: e.to_string(),
                    });
                }
            }
        }

        Ok((valid, diags))
    }

    // ── history ───────────────────────────────────────────────────────────────

    /// Read all history revisions for an entity (oldest first).
    pub fn read_history(&self, entity_path: &Path) -> Result<Vec<HistoryRevision>, StorageError> {
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
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{}", line)?;
        Ok(rev)
    }

    /// Ensure the assets subdirectory exists inside `entity_path`.
    pub fn ensure_assets_dir(&self, entity_path: &Path) -> Result<(), StorageError> {
        let assets = entity_path.join(self.config.assets_dir);
        if !assets.exists() {
            fs::create_dir_all(&assets)?;
        }
        Ok(())
    }

    /// Reformat an existing entity's manifest to canonical field ordering.
    pub fn reformat(&self, entity_path: &Path) -> Result<(), StorageError> {
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

    /// Write or overwrite the `description.md` file for an entity.
    pub fn write_description(&self, entity_path: &Path, text: &str) -> Result<(), StorageError> {
        let lock_path = entity_path.join(self.config.lock_file);
        let lock_file = acquire_lock(&lock_path)?;
        let result = fs::write(entity_path.join("description.md"), text).map_err(StorageError::Io);
        release_lock(&lock_file, &lock_path);
        result
    }

    /// Read text content of `description.md`. Returns `None` if not present.
    pub fn read_description(&self, entity_path: &Path) -> Option<String> {
        let desc = entity_path.join("description.md");
        fs::read_to_string(&desc).ok()
    }

    // ── internal ──────────────────────────────────────────────────────────────

    fn write_manifest(&self, dir: &Path, manifest: &EntityManifest) -> Result<(), StorageError> {
        let toml_str = crate::model::manifest_format::format_manifest_toml(manifest);
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

fn release_lock(file: &File, lock_path: &Path) {
    let _ = file.unlock();
    let _ = fs::remove_file(lock_path);
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::HistoryRevision;
    use std::collections::BTreeMap;
    use serde_json::Value;

    #[test]
    fn history_revision_backward_compat_no_author() {
        let json = r#"{"rev":1,"ts":"2025-01-01T00:00:00Z","fields":{"state":"new","title":"Old entry"}}"#;
        let rev: HistoryRevision = serde_json::from_str(json)
            .expect("should deserialize legacy revision without author field");
        assert_eq!(rev.rev, 1);
        assert_eq!(rev.author, None, "author should be None for legacy entries");
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
        assert!(v.get("author").is_none(), "author key should be absent when None");
    }
}
