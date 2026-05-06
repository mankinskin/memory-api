use std::fs;
use std::path::{
    Path,
    PathBuf,
};
use std::time::UNIX_EPOCH;

use chrono::Utc;
use ignore::WalkBuilder;
use rusqlite::{
    Connection,
    OptionalExtension,
    params,
};
use sha2::{
    Digest,
    Sha256,
};

use crate::config::{
    format_output_path,
    is_repo_relative_path_excluded,
};
use crate::error::AuditError;
use crate::models::{
    AuditFinding,
    AuditMetrics,
    IndexedFile,
    SyncStats,
};

const INDEX_DIR: &str = ".audit";
const INDEX_DB: &str = "audit.sqlite3";

pub struct RepositoryIndex {
    repo_root: PathBuf,
    db_path: PathBuf,
}

impl RepositoryIndex {
    pub fn open(repo_root: &Path) -> Result<Self, AuditError> {
        if !repo_root.exists() {
            return Err(AuditError::MissingRepoRoot(format_output_path(repo_root)));
        }

        let index_dir = repo_root.join(INDEX_DIR);
        fs::create_dir_all(&index_dir)?;
        let db_path = index_dir.join(INDEX_DB);

        let this = Self {
            repo_root: repo_root.to_path_buf(),
            db_path,
        };
        let conn = this.connect()?;
        this.init_schema(&conn)?;
        Ok(this)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn sync_source_files(
        &self,
        exclude_paths: &[String],
    ) -> Result<SyncStats, AuditError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let scan_token = Utc::now().format("%Y%m%d%H%M%S%3f").to_string();
        let mut scanned_files = 0usize;
        let mut updated_files = 0usize;
        let mut reused_files = 0usize;

        let mut walker = WalkBuilder::new(&self.repo_root);
        walker.standard_filters(true);
        walker.hidden(false);
        let repo_root = self.repo_root.clone();
        let exclude_paths = exclude_paths.to_vec();
        walker.filter_entry(move |entry| {
            let Ok(relative_path) = entry.path().strip_prefix(&repo_root) else {
                return true;
            };
            !is_excluded_path(relative_path, &exclude_paths)
        });

        for entry in walker.build() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let path = entry.path();

            if !entry.file_type().is_some_and(|file_type| file_type.is_file()) {
                continue;
            }

            let Some(language) = detect_language(path) else {
                continue;
            };

            let relative_path = match path.strip_prefix(&self.repo_root) {
                Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            let metadata = fs::metadata(path)?;
            let modified_unix_ms = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or_default();
            let size_bytes = metadata.len();

            scanned_files += 1;

            let existing: Option<(i64, u64)> = tx
                .query_row(
                    "SELECT modified_unix_ms, size_bytes FROM files WHERE path = ?1",
                    params![relative_path],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            if existing
                .as_ref()
                .is_some_and(|(existing_modified, existing_size)| {
                    *existing_modified == modified_unix_ms && *existing_size == size_bytes
                })
            {
                tx.execute(
                    "UPDATE files SET last_scan_token = ?1 WHERE path = ?2",
                    params![scan_token, relative_path],
                )?;
                reused_files += 1;
                continue;
            }

            let content = fs::read(path)?;
            let line_count = count_lines(&content);
            let sha256 = format!("{:x}", Sha256::digest(&content));

            tx.execute(
                "INSERT INTO files (
                    path,
                    language,
                    size_bytes,
                    modified_unix_ms,
                    sha256,
                    line_count,
                    last_scan_token
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(path) DO UPDATE SET
                    language = excluded.language,
                    size_bytes = excluded.size_bytes,
                    modified_unix_ms = excluded.modified_unix_ms,
                    sha256 = excluded.sha256,
                    line_count = excluded.line_count,
                    last_scan_token = excluded.last_scan_token",
                params![
                    relative_path,
                    language,
                    size_bytes,
                    modified_unix_ms,
                    sha256,
                    line_count,
                    scan_token,
                ],
            )?;
            updated_files += 1;
        }

        let pruned_files = tx.execute(
            "DELETE FROM files WHERE last_scan_token != ?1",
            params![scan_token],
        )?;

        tx.commit()?;

        Ok(SyncStats {
            scan_token,
            scanned_files,
            updated_files,
            reused_files,
            pruned_files,
        })
    }

    pub fn indexed_files(&self) -> Result<Vec<IndexedFile>, AuditError> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            "SELECT path, language, size_bytes, modified_unix_ms, sha256, line_count
             FROM files
             ORDER BY path ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(IndexedFile {
                path: row.get(0)?,
                language: row.get(1)?,
                size_bytes: row.get(2)?,
                modified_unix_ms: row.get(3)?,
                sha256: row.get(4)?,
                line_count: row.get(5)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn record_audit_run(
        &self,
        started_at: &str,
        finished_at: &str,
        status: &str,
        metrics: &AuditMetrics,
        sync: &SyncStats,
        findings: &[AuditFinding],
    ) -> Result<i64, AuditError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let metrics_json = serde_json::to_string(metrics)?;
        let sync_json = serde_json::to_string(sync)?;

        tx.execute(
            "INSERT INTO audit_runs (
                repo_root,
                started_at,
                finished_at,
                status,
                metrics_json,
                sync_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                format_output_path(&self.repo_root),
                started_at,
                finished_at,
                status,
                metrics_json,
                sync_json,
            ],
        )?;
        let run_id = tx.last_insert_rowid();

        for finding in findings {
            tx.execute(
                "INSERT INTO audit_findings (
                    run_id,
                    finding_id,
                    category,
                    severity,
                    summary,
                    path,
                    line,
                    metric_name,
                    metric_value_json,
                    threshold_json,
                    instructions_json,
                    evidence_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    run_id,
                    finding.id,
                    finding.category,
                    format!("{:?}", finding.severity).to_lowercase(),
                    finding.summary,
                    finding.path,
                    finding.line.map(|line| line as i64),
                    finding.metric_name,
                    serde_json::to_string(&finding.metric_value)?,
                    finding.threshold.as_ref().map(serde_json::to_string).transpose()?,
                    serde_json::to_string(&finding.instructions)?,
                    serde_json::to_string(&finding.evidence)?,
                ],
            )?;
        }

        tx.commit()?;
        Ok(run_id)
    }

    fn connect(&self) -> Result<Connection, AuditError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;",
        )?;
        Ok(conn)
    }

    fn init_schema(&self, conn: &Connection) -> Result<(), AuditError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                language TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                modified_unix_ms INTEGER NOT NULL,
                sha256 TEXT NOT NULL,
                line_count INTEGER NOT NULL,
                last_scan_token TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS audit_runs (
                run_id INTEGER PRIMARY KEY AUTOINCREMENT,
                repo_root TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT NOT NULL,
                status TEXT NOT NULL,
                metrics_json TEXT NOT NULL,
                sync_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS audit_findings (
                run_id INTEGER NOT NULL,
                finding_id TEXT NOT NULL,
                category TEXT NOT NULL,
                severity TEXT NOT NULL,
                summary TEXT NOT NULL,
                path TEXT,
                line INTEGER,
                metric_name TEXT NOT NULL,
                metric_value_json TEXT NOT NULL,
                threshold_json TEXT,
                instructions_json TEXT NOT NULL,
                evidence_json TEXT NOT NULL,
                PRIMARY KEY (run_id, finding_id),
                FOREIGN KEY (run_id) REFERENCES audit_runs(run_id) ON DELETE CASCADE
            );",
        )?;
        Ok(())
    }
}

fn is_excluded_path(
    relative_path: &Path,
    exclude_paths: &[String],
) -> bool {
    if relative_path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        matches!(
            value.as_ref(),
            ".git" | "target" | "node_modules" | ".audit" | ".idea" | ".vscode"
        )
    }) {
        return true;
    }

    is_repo_relative_path_excluded(relative_path, exclude_paths)
}

fn detect_language(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_string_lossy().to_ascii_lowercase().as_str() {
        "rs" => Some("rust"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" => Some("javascript"),
        "py" => Some("python"),
        "go" => Some("go"),
        "java" => Some("java"),
        "kt" => Some("kotlin"),
        "c" | "h" => Some("c"),
        "cc" | "cpp" | "cxx" | "hpp" => Some("cpp"),
        _ => None,
    }
}

fn count_lines(content: &[u8]) -> usize {
    if content.is_empty() {
        0
    } else {
        String::from_utf8_lossy(content).lines().count()
    }
}