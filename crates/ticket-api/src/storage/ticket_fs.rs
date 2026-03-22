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
use crate::model::filesystem::{
    TICKET_ASSETS_DIR, TICKET_HISTORY_FILE, TICKET_LOCK_FILE, TICKET_MANIFEST_FILE,
    parse_ticket_manifest_toml,
};
use crate::model::ticket::TicketManifest;

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
}

/// Low-level filesystem operations for ticket folders.
///
/// Each ticket lives in a folder named by its UUID:
///
/// ```text
/// <scan_root>/<uuid>/
///   ticket.toml         ← manifest (TOML)
///   .ticket-lock        ← advisory lock file (held during writes)
///   assets/             ← optional attachments
/// ```
pub struct TicketFs;

impl TicketFs {
    /// Create a new ticket folder under `target_root`.
    ///
    /// Protocol:
    /// 1. Write manifest to a temp folder `<uuid>.tmp/`
    /// 2. Rename temp → final `<uuid>/` (atomic on POSIX; best-effort on Windows)
    ///
    /// Returns the absolute path to the created ticket folder.
    pub fn create(
        manifest: &TicketManifest,
        target_root: &Path,
        body: Option<&str>,
    ) -> Result<PathBuf, StorageError> {
        let uuid_str = manifest.id.to_string();
        let final_dir = target_root.join(&uuid_str);
        let temp_dir = target_root.join(format!("{}.tmp", uuid_str));

        if final_dir.exists() {
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("ticket folder already exists: {}", final_dir.display()),
            )));
        }

        // Write to temp dir first.
        fs::create_dir_all(&temp_dir)?;
        write_manifest(&temp_dir, manifest)?;
        if let Some(text) = body {
            fs::write(temp_dir.join("description.md"), text)?;
        }

        // Rename temp → final.
        fs::rename(&temp_dir, &final_dir).map_err(|e| {
            // Clean up temp on failure.
            let _ = fs::remove_dir_all(&temp_dir);
            StorageError::Io(e)
        })?;

        Ok(final_dir)
    }

    /// Read and parse the manifest from an existing ticket folder.
    pub fn read(ticket_path: &Path) -> Result<TicketManifest, StorageError> {
        let manifest_path = ticket_path.join(TICKET_MANIFEST_FILE);
        let content = fs::read_to_string(&manifest_path)?;
        parse_ticket_manifest_toml(manifest_path.clone(), &content).map_err(|d| {
            StorageError::ParseError {
                path: d.path,
                reason: d.reason,
            }
        })
    }

    /// Apply a field patch to the manifest on disk.
    ///
    /// Protocol:
    /// 1. Acquire `.ticket-lock` (exclusive)
    /// 2. Read + merge patch
    /// 3. Write updated `ticket.toml`
    /// 4. Release lock
    ///
    /// Returns the updated manifest.
    pub fn update(
        ticket_path: &Path,
        patch: &std::collections::BTreeMap<String, Value>,
        new_state: Option<&str>,
    ) -> Result<TicketManifest, StorageError> {
        let lock_path = ticket_path.join(TICKET_LOCK_FILE);
        let lock_file = acquire_lock(&lock_path)?;

        let result = (|| -> Result<TicketManifest, StorageError> {
            let mut manifest = Self::read(ticket_path)?;
            // Apply extra-field patches.
            for (k, v) in patch {
                manifest.extra.insert(k.clone(), v.clone());
            }
            // Apply state change.
            if let Some(state) = new_state {
                manifest
                    .extra
                    .insert("state".to_string(), Value::String(state.to_string()));
            }
            write_manifest(ticket_path, &manifest)?;
            Ok(manifest)
        })();

        release_lock(&lock_file);
        result
    }

    /// Soft-delete a ticket by writing a `deleted = true` marker in the manifest.
    /// The folder is not removed.
    pub fn mark_deleted(ticket_path: &Path) -> Result<(), StorageError> {
        let lock_path = ticket_path.join(TICKET_LOCK_FILE);
        let lock_file = acquire_lock(&lock_path)?;

        let result = (|| -> Result<(), StorageError> {
            let mut manifest = Self::read(ticket_path)?;
            manifest
                .extra
                .insert("deleted".to_string(), Value::Bool(true));
            write_manifest(ticket_path, &manifest)?;
            Ok(())
        })();

        release_lock(&lock_file);
        result
    }

    /// Walk `scan_root` and locate all valid ticket folders.
    ///
    /// A folder is considered a valid ticket folder if it:
    /// - Has a UUID-parseable name, **and**
    /// - Contains a `ticket.toml` file
    ///
    /// Returns `(valid_paths, parse_diagnostics)`.
    pub fn scan_root(
        scan_root: &Path,
    ) -> Result<(Vec<TicketScanEntry>, Vec<crate::model::filesystem::ParseDiagnostic>), StorageError>
    {
        let mut valid = Vec::new();
        let mut diags = Vec::new();

        let read_dir = match fs::read_dir(scan_root) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((valid, diags)),
            Err(e) => return Err(StorageError::Io(e)),
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip temp and deleted folders.
            if name.ends_with(".tmp") || name.ends_with(".deleted") {
                continue;
            }

            // Must be UUID-parseable.
            let id: Uuid = match name.parse() {
                Ok(u) => u,
                Err(_) => continue,
            };

            let manifest_path = path.join(TICKET_MANIFEST_FILE);
            if !manifest_path.exists() {
                diags.push(crate::model::filesystem::ParseDiagnostic {
                    path: manifest_path,
                    reason: "missing ticket.toml".to_string(),
                });
                continue;
            }

            match Self::read(&path) {
                Ok(manifest) => {
                    // Skip tickets whose manifest has been soft-deleted.
                    let is_deleted = manifest
                        .extra
                        .get("deleted")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if !is_deleted {
                        valid.push(TicketScanEntry { id, path, manifest });
                    }
                }
                Err(StorageError::ParseError { path: p, reason }) => {
                    diags.push(crate::model::filesystem::ParseDiagnostic {
                        path: p,
                        reason,
                    });
                }
                Err(e) => {
                    diags.push(crate::model::filesystem::ParseDiagnostic {
                        path: manifest_path,
                        reason: e.to_string(),
                    });
                }
            }
        }

        Ok((valid, diags))
    }

    // ── history ───────────────────────────────────────────────────────────────

    /// Read all history revisions for a ticket (oldest first).
    ///
    /// Returns an empty vec if no `history.ndjson` exists yet.
    pub fn read_history(ticket_path: &Path) -> Result<Vec<HistoryRevision>, StorageError> {
        let path = ticket_path.join(TICKET_HISTORY_FILE);
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
    ///
    /// The revision number is `existing_count + 1`.
    pub fn append_history(
        ticket_path: &Path,
        fields: BTreeMap<String, Value>,
    ) -> Result<u64, StorageError> {
        let path = ticket_path.join(TICKET_HISTORY_FILE);
        // Count existing revisions to assign the next rev number.
        let existing_count = Self::read_history(ticket_path)?.len() as u64;
        let rev = existing_count + 1;
        let entry = HistoryRevision {
            rev,
            ts: Utc::now().to_rfc3339(),
            fields,
        };
        let line = serde_json::to_string(&entry)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{}", line)?;
        Ok(rev)
    }

    /// Ensure the `assets/` subdirectory exists inside `ticket_path`.
    pub fn ensure_assets_dir(ticket_path: &Path) -> Result<(), StorageError> {
        let assets = ticket_path.join(TICKET_ASSETS_DIR);
        if !assets.exists() {
            fs::create_dir_all(&assets)?;
        }
        Ok(())
    }

    /// Read text content of a file inside the assets directory for search indexing.
    /// Returns `None` if no `description.md` exists.
    pub fn read_description(ticket_path: &Path) -> Option<String> {
        let desc = ticket_path.join("description.md");
        fs::read_to_string(&desc).ok()
    }
}

pub struct TicketScanEntry {
    pub id: Uuid,
    pub path: PathBuf,
    pub manifest: TicketManifest,
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn write_manifest(dir: &Path, manifest: &TicketManifest) -> Result<(), StorageError> {
    let toml_str = toml::to_string_pretty(manifest)
        .map_err(|e| StorageError::Serialization(e.to_string()))?;
    let path = dir.join(TICKET_MANIFEST_FILE);
    fs::write(&path, toml_str)?;
    Ok(())
}

fn acquire_lock(lock_path: &Path) -> Result<File, StorageError> {
    let file = File::create(lock_path)?;
    file.lock_exclusive()
        .map_err(|e| StorageError::Io(e))?;
    Ok(file)
}

fn release_lock(file: &File) {
    let _ = file.unlock();
}
