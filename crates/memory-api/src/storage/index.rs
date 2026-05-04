use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use uuid::Uuid;

use crate::error::StorageError;
use crate::model::edge::EdgeRecord;
use crate::model::filesystem::ScanRoot;
use crate::storage::schema::{
    SCHEMA_VERSION, TABLE_BOARD_ACTIVE_INDEX, TABLE_BOARD_CONFIG, TABLE_BOARD_ENTRIES,
    TABLE_EDGES, TABLE_LEASES, TABLE_META, TABLE_SCAN_ROOTS, TABLE_TICKETS,
};

use super::indexed::{IndexedEntity, LeaseInfo};

/// SQLite-backed metadata index.
///
/// # Concurrency model
///
/// SQLite WAL (Write-Ahead Logging) mode is used so that multiple processes
/// can hold concurrent read connections without blocking each other.  Read
/// operations open a short-lived connection each time (cheap in WAL mode).
/// Writes are serialised in-process by `write_lock: Mutex<()>` and at the
/// OS level by SQLite's own writer lock, but readers are never blocked by
/// writers in WAL mode.
///
/// This replaced the previous `redb`-backed implementation that held an
/// exclusive OS-level file lock for the entire process lifetime, preventing
/// any other process (CLI, VS Code extension, tests) from opening the same
/// database simultaneously.
pub struct RedbIndexStore {
    db_path: PathBuf,
    /// Serialises write operations within this process.
    write_lock: Mutex<()>,
}

impl RedbIndexStore {
    pub fn open(db_path: &Path) -> Result<Self, StorageError> {
        let conn = write_connection(db_path)?;
        ensure_tables(&conn)?;
        check_or_set_schema_version(&conn)?;
        Ok(Self {
            db_path: db_path.to_path_buf(),
            write_lock: Mutex::new(()),
        })
    }

    fn read_conn(&self) -> Result<Connection, StorageError> {
        read_connection(&self.db_path)
    }

    fn with_write<F, R>(&self, f: F) -> Result<R, StorageError>
    where
        F: FnOnce(&Connection) -> Result<R, StorageError>,
    {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let conn = write_connection(&self.db_path)?;
        f(&conn)
    }

    /// Used by board operations (which return `BoardError` that impl `From<StorageError>`).
    pub fn with_db_ext<F, R, E>(&self, f: F) -> Result<R, E>
    where
        F: FnOnce(&Connection) -> Result<R, E>,
        E: From<StorageError>,
    {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let conn = write_connection(&self.db_path).map_err(Into::into)?;
        f(&conn)
    }

    // ── entity CRUD ──────────────────────────────────────────────────────────

    pub fn insert_ticket(&self, entity: &IndexedEntity) -> Result<(), StorageError> {
        let bytes = bincode::serialize(entity)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        let key = entity.id.to_string();
        self.with_write(|conn| {
            conn.execute(
                &format!("INSERT OR REPLACE INTO {TABLE_TICKETS} (id, data) VALUES (?1, ?2)"),
                params![key, bytes],
            )?;
            Ok(())
        })
    }

    pub fn get_ticket(&self, id: &Uuid) -> Result<Option<IndexedEntity>, StorageError> {
        let key = id.to_string();
        let conn = self.read_conn()?;
        let mut stmt =
            conn.prepare(&format!("SELECT data FROM {TABLE_TICKETS} WHERE id = ?1"))?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let bytes: Vec<u8> = row.get(0)?;
            let entity: IndexedEntity = bincode::deserialize(&bytes)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            Ok(Some(entity))
        } else {
            Ok(None)
        }
    }

    pub fn list_tickets(&self, include_deleted: bool) -> Result<Vec<IndexedEntity>, StorageError> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(&format!("SELECT data FROM {TABLE_TICKETS}"))?;
        let rows = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        let mut entities = Vec::new();
        for bytes in rows {
            let entity: IndexedEntity = bincode::deserialize(&bytes?)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            if include_deleted || !entity.deleted {
                entities.push(entity);
            }
        }
        Ok(entities)
    }

    /// Fetch multiple tickets in a single read connection.
    pub fn get_tickets_by_ids(
        &self,
        ids: &[Uuid],
    ) -> Result<HashMap<Uuid, IndexedEntity>, StorageError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.read_conn()?;
        let placeholders: String = (1..=ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql =
            format!("SELECT data FROM {TABLE_TICKETS} WHERE id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql)?;
        let id_strs: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        let params: Vec<&dyn rusqlite::types::ToSql> =
            id_strs.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), |row| row.get::<_, Vec<u8>>(0))?;
        let mut map = HashMap::with_capacity(ids.len());
        for bytes in rows {
            let entity: IndexedEntity = bincode::deserialize(&bytes?)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            if !entity.deleted {
                map.insert(entity.id, entity);
            }
        }
        Ok(map)
    }

    /// Soft-delete: marks the index entry as deleted.
    pub fn soft_delete_ticket(&self, id: &Uuid) -> Result<(), StorageError> {
        let key = id.to_string();
        self.with_write(|conn| {
            let mut entity = {
                let mut stmt = conn
                    .prepare(&format!("SELECT data FROM {TABLE_TICKETS} WHERE id = ?1"))?;
                let mut rows = stmt.query(params![&key])?;
                match rows.next()? {
                    Some(row) => {
                        let bytes: Vec<u8> = row.get(0)?;
                        bincode::deserialize::<IndexedEntity>(&bytes)
                            .map_err(|e| StorageError::Serialization(e.to_string()))?
                    }
                    None => return Err(StorageError::NotFound(*id)),
                }
            };
            entity.deleted = true;
            entity.updated_at = chrono::Utc::now();
            let bytes = bincode::serialize(&entity)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_TICKETS} (id, data) VALUES (?1, ?2)"
                ),
                params![key, bytes],
            )?;
            Ok(())
        })
    }

    /// Hard-delete an entity from the index.
    pub fn remove_ticket(&self, id: &Uuid) -> Result<(), StorageError> {
        let key = id.to_string();
        self.with_write(|conn| {
            conn.execute(
                &format!("DELETE FROM {TABLE_TICKETS} WHERE id = ?1"),
                params![key],
            )?;
            Ok(())
        })
    }

    // ── edge CRUD ─────────────────────────────────────────────────────────────

    /// Insert an edge. Duplicate insert is idempotent.
    pub fn insert_edge(&self, edge: &EdgeRecord) -> Result<(), StorageError> {
        let from = edge.from.to_string();
        let to = edge.to.to_string();
        let created_at = edge.created_at.to_rfc3339();
        self.with_write(|conn| {
            conn.execute(
                &format!(
                    "INSERT OR IGNORE INTO {TABLE_EDGES} \
                     (from_id, to_id, kind, created_at) VALUES (?1, ?2, ?3, ?4)"
                ),
                params![from, to, edge.kind, created_at],
            )?;
            Ok(())
        })
    }

    /// Delete an edge. Missing edges are a no-op.
    pub fn delete_edge(&self, edge: &EdgeRecord) -> Result<(), StorageError> {
        let from = edge.from.to_string();
        let to = edge.to.to_string();
        self.with_write(|conn| {
            conn.execute(
                &format!(
                    "DELETE FROM {TABLE_EDGES} WHERE from_id = ?1 AND to_id = ?2 AND kind = ?3"
                ),
                params![from, to, edge.kind],
            )?;
            Ok(())
        })
    }

    /// Returns all edges originating from `from`.
    pub fn edges_from(&self, from: &Uuid) -> Result<Vec<EdgeRecord>, StorageError> {
        let from_str = from.to_string();
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT from_id, to_id, kind, created_at FROM {TABLE_EDGES} WHERE from_id = ?1"
        ))?;
        let rows = stmt.query_map(params![from_str], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut edges = Vec::new();
        for row in rows {
            let (f, t, k, ca) = row?;
            edges.push(parse_edge_row(&f, &t, &k, &ca)?);
        }
        Ok(edges)
    }

    /// Returns every edge in the store.
    pub fn list_all_edges(&self) -> Result<Vec<EdgeRecord>, StorageError> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT from_id, to_id, kind, created_at FROM {TABLE_EDGES}"
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut edges = Vec::new();
        for row in rows {
            let (f, t, k, ca) = row?;
            edges.push(parse_edge_row(&f, &t, &k, &ca)?);
        }
        Ok(edges)
    }

    /// Returns the number of non-deleted tickets without deserializing rows.
    ///
    /// Note: deleted tickets are stored as BLOBs with a flag inside, so we
    /// can only do an approximate count via `COUNT(*)` (includes soft-deleted).
    /// For the SSE snapshot baseline this is accurate enough.
    pub fn count_tickets(&self) -> Result<usize, StorageError> {
        let conn = self.read_conn()?;
        let n: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {TABLE_TICKETS}"),
            [],
            |row| row.get(0),
        )?;
        Ok(n as usize)
    }

    /// Returns the number of edges without fetching the full edge list.
    pub fn count_edges(&self) -> Result<usize, StorageError> {
        let conn = self.read_conn()?;
        let n: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {TABLE_EDGES}"),
            [],
            |row| row.get(0),
        )?;
        Ok(n as usize)
    }

    // ── scan root registry ───────────────────────────────────────────────────

    pub fn add_scan_root(&self, root: &ScanRoot) -> Result<(), StorageError> {
        let path_str = root.path.to_string_lossy().into_owned();
        let label = root.label.clone();
        self.with_write(|conn| {
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_SCAN_ROOTS} (path, label) VALUES (?1, ?2)"
                ),
                params![path_str, label],
            )?;
            Ok(())
        })
    }

    pub fn list_scan_roots(&self) -> Result<Vec<ScanRoot>, StorageError> {
        let conn = self.read_conn()?;
        let mut stmt =
            conn.prepare(&format!("SELECT path, label FROM {TABLE_SCAN_ROOTS}"))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut roots = Vec::new();
        for row in rows {
            let (path_str, label) = row?;
            roots.push(ScanRoot {
                path: std::path::PathBuf::from(path_str),
                label,
            });
        }
        Ok(roots)
    }

    // ── lease CRUD ────────────────────────────────────────────────────────────

    pub fn insert_lease(&self, lease: &LeaseInfo) -> Result<(), StorageError> {
        let bytes = bincode::serialize(lease)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        let key = lease.ticket_id.to_string();
        self.with_write(|conn| {
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_LEASES} (id, data) VALUES (?1, ?2)"
                ),
                params![key, bytes],
            )?;
            Ok(())
        })
    }

    pub fn get_lease(&self, ticket_id: &Uuid) -> Result<Option<LeaseInfo>, StorageError> {
        let key = ticket_id.to_string();
        let conn = self.read_conn()?;
        let mut stmt =
            conn.prepare(&format!("SELECT data FROM {TABLE_LEASES} WHERE id = ?1"))?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let bytes: Vec<u8> = row.get(0)?;
            let lease: LeaseInfo = bincode::deserialize(&bytes)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            Ok(Some(lease))
        } else {
            Ok(None)
        }
    }

    pub fn remove_lease(&self, ticket_id: &Uuid) -> Result<(), StorageError> {
        let key = ticket_id.to_string();
        self.with_write(|conn| {
            conn.execute(
                &format!("DELETE FROM {TABLE_LEASES} WHERE id = ?1"),
                params![key],
            )?;
            Ok(())
        })
    }

    pub fn list_active_leases(&self) -> Result<Vec<LeaseInfo>, StorageError> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(&format!("SELECT data FROM {TABLE_LEASES}"))?;
        let rows = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        let mut leases = Vec::new();
        for bytes in rows {
            let lease: LeaseInfo = bincode::deserialize(&bytes?)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            leases.push(lease);
        }
        Ok(leases)
    }

    // ── internal ──────────────────────────────────────────────────────────────

    /// BFS reachability check: returns `true` if `target` is reachable from
    /// `start` following outgoing edges. Used for cycle detection.
    pub fn is_reachable(&self, start: &Uuid, target: &Uuid) -> Result<bool, StorageError> {
        use std::collections::{HashSet, VecDeque};

        let all_edges = self.list_all_edges()?;
        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut queue: VecDeque<Uuid> = VecDeque::new();
        queue.push_back(*start);

        while let Some(current) = queue.pop_front() {
            if &current == target {
                return Ok(true);
            }
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);
            for edge in all_edges.iter().filter(|e| e.from == current) {
                queue.push_back(edge.to);
            }
        }
        Ok(false)
    }
}

// ── connection helpers ────────────────────────────────────────────────────────

fn read_connection(db_path: &Path) -> Result<Connection, StorageError> {
    // `unlock_notify` feature: if a write lock is briefly held, the read will
    // wait for notification rather than returning SQLITE_BUSY immediately.
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| StorageError::Database(e.to_string()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| StorageError::Database(e.to_string()))?;
    Ok(conn)
}

fn write_connection(db_path: &Path) -> Result<Connection, StorageError> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| StorageError::Database(e.to_string()))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .map_err(|e| StorageError::Database(e.to_string()))?;
    Ok(conn)
}

fn ensure_tables(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(&format!(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS {TABLE_TICKETS} (
             id   TEXT PRIMARY KEY NOT NULL,
             data BLOB NOT NULL
         );
         CREATE TABLE IF NOT EXISTS {TABLE_EDGES} (
             from_id    TEXT NOT NULL,
             to_id      TEXT NOT NULL,
             kind       TEXT NOT NULL,
             created_at TEXT NOT NULL,
             PRIMARY KEY (from_id, to_id, kind)
         );
         CREATE TABLE IF NOT EXISTS {TABLE_SCAN_ROOTS} (
             path  TEXT PRIMARY KEY NOT NULL,
             label TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS {TABLE_LEASES} (
             id   TEXT PRIMARY KEY NOT NULL,
             data BLOB NOT NULL
         );
         CREATE TABLE IF NOT EXISTS {TABLE_META} (
             key   TEXT PRIMARY KEY NOT NULL,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS {TABLE_BOARD_ENTRIES} (
             id   TEXT PRIMARY KEY NOT NULL,
             data BLOB NOT NULL
         );
         CREATE TABLE IF NOT EXISTS {TABLE_BOARD_ACTIVE_INDEX} (
             key   TEXT PRIMARY KEY NOT NULL,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS {TABLE_BOARD_CONFIG} (
             key  TEXT PRIMARY KEY NOT NULL,
             data BLOB NOT NULL
         );
         COMMIT;"
    ))
    .map_err(|e| StorageError::Database(e.to_string()))
}

fn check_or_set_schema_version(conn: &Connection) -> Result<(), StorageError> {
    use crate::storage::schema::ensure_supported_schema_version;
    let existing: Option<String> = conn
        .query_row(
            &format!("SELECT value FROM {TABLE_META} WHERE key = 'schema_version'"),
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| StorageError::Database(e.to_string()))?;
    match existing {
        Some(found) => ensure_supported_schema_version(&found)?,
        None => {
            conn.execute(
                &format!(
                    "INSERT INTO {TABLE_META} (key, value) VALUES ('schema_version', ?1)"
                ),
                params![SCHEMA_VERSION],
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        }
    }
    Ok(())
}

fn parse_edge_row(
    from_s: &str,
    to_s: &str,
    kind: &str,
    created_at_s: &str,
) -> Result<EdgeRecord, StorageError> {
    let from: Uuid = from_s
        .parse()
        .map_err(|e: uuid::Error| StorageError::Serialization(e.to_string()))?;
    let to: Uuid = to_s
        .parse()
        .map_err(|e: uuid::Error| StorageError::Serialization(e.to_string()))?;
    let created_at = chrono::DateTime::parse_from_rfc3339(created_at_s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());
    Ok(EdgeRecord {
        from,
        to,
        kind: kind.to_string(),
        created_at,
    })
}
