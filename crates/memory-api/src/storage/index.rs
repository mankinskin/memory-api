use std::path::{Path, PathBuf};

use redb::{Database, ReadableTable, TableDefinition};
use uuid::Uuid;

use crate::error::StorageError;
use crate::model::edge::EdgeRecord;
use crate::model::filesystem::ScanRoot;
use crate::storage::schema::{SCHEMA_VERSION, TABLE_EDGES, TABLE_LEASES, TABLE_META, TABLE_SCAN_ROOTS, TABLE_TICKETS};

use super::indexed::{IndexedEntity, LeaseInfo};

const TICKETS: TableDefinition<&str, &[u8]> = TableDefinition::new(TABLE_TICKETS);
const EDGES: TableDefinition<&str, ()> = TableDefinition::new(TABLE_EDGES);
const SCAN_ROOTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new(TABLE_SCAN_ROOTS);
const LEASES: TableDefinition<&str, &[u8]> = TableDefinition::new(TABLE_LEASES);
const META: TableDefinition<&str, &str> = TableDefinition::new(TABLE_META);

/// Redb-backed metadata index.
///
/// Keeps the [`Database`] open for the lifetime of the store. ReDB 2.x
/// is `Send + Sync` and handles concurrent access internally: read
/// transactions run in parallel; write transactions are serialised by
/// ReDB itself via `begin_write()`. No application-level lock is needed.
pub struct RedbIndexStore {
    db_path: PathBuf,
    db: Database,
}

impl RedbIndexStore {
    pub fn open(db_path: &Path) -> Result<Self, StorageError> {
        let db = open_db(db_path)?;
        ensure_tables(&db)?;
        check_or_set_schema_version(&db)?;
        Ok(Self {
            db_path: db_path.to_path_buf(),
            db,
        })
    }

    fn with_db<F, R>(&self, f: F) -> Result<R, StorageError>
    where
        F: FnOnce(&Database) -> Result<R, StorageError>,
    {
        f(&self.db)
    }

    /// Generic variant of `with_db` accepting any error type `E` that can be
    /// constructed from [`StorageError`]. Used by the board extension impl.
    pub fn with_db_ext<F, R, E>(&self, f: F) -> Result<R, E>
    where
        F: FnOnce(&Database) -> Result<R, E>,
        E: From<StorageError>,
    {
        f(&self.db)
    }

    // ── entity CRUD ──────────────────────────────────────────────────────────

    pub fn insert_ticket(&self, entity: &IndexedEntity) -> Result<(), StorageError> {
        let bytes = bincode::serialize(entity)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        let key = entity.id.to_string();
        self.with_db(|db| {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(TICKETS)?;
                table.insert(key.as_str(), bytes.as_slice())?;
            }
            write_txn.commit()?;
            Ok(())
        })
    }

    pub fn get_ticket(&self, id: &Uuid) -> Result<Option<IndexedEntity>, StorageError> {
        let key = id.to_string();
        self.with_db(|db| {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(TICKETS)?;
            match table.get(key.as_str())? {
                Some(value) => {
                    let entity: IndexedEntity = bincode::deserialize(value.value())
                        .map_err(|e| StorageError::Serialization(e.to_string()))?;
                    Ok(Some(entity))
                }
                None => Ok(None),
            }
        })
    }

    pub fn list_tickets(&self, include_deleted: bool) -> Result<Vec<IndexedEntity>, StorageError> {
        self.with_db(|db| {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(TICKETS)?;
            let mut entities = Vec::new();
            for result in table.iter()? {
                let (_, value) = result?;
                let entity: IndexedEntity = bincode::deserialize(value.value())
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                if include_deleted || !entity.deleted {
                    entities.push(entity);
                }
            }
            Ok(entities)
        })
    }

    /// Soft-delete: marks the index entry as deleted. Filesystem folder is not touched here.
    pub fn soft_delete_ticket(&self, id: &Uuid) -> Result<(), StorageError> {
        let key = id.to_string();
        self.with_db(|db| {
            let mut entity = {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(TICKETS)?;
                match table.get(key.as_str())? {
                    Some(value) => bincode::deserialize::<IndexedEntity>(value.value())
                        .map_err(|e| StorageError::Serialization(e.to_string()))?,
                    None => return Err(StorageError::NotFound(*id)),
                }
            };
            entity.deleted = true;
            entity.updated_at = chrono::Utc::now();
            let bytes = bincode::serialize(&entity)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(TICKETS)?;
                table.insert(key.as_str(), bytes.as_slice())?;
            }
            write_txn.commit()?;
            Ok(())
        })
    }

    /// Hard-delete an entity entry from the redb index.
    pub fn remove_ticket(&self, id: &Uuid) -> Result<(), StorageError> {
        let key = id.to_string();
        self.with_db(|db| {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(TICKETS)?;
                table.remove(key.as_str())?;
            }
            write_txn.commit()?;
            Ok(())
        })
    }

    // ── edge CRUD ─────────────────────────────────────────────────────────────

    /// Insert an edge using `"{from}|{to}|{kind}"` as the composite key.
    /// Duplicate insert is idempotent.
    pub fn insert_edge(&self, edge: &EdgeRecord) -> Result<(), StorageError> {
        let key = format!("{}|{}|{}", edge.from, edge.to, edge.kind);
        self.with_db(|db| {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(EDGES)?;
                table.insert(key.as_str(), ())?;
            }
            write_txn.commit()?;
            Ok(())
        })
    }

    /// Delete an edge by composite key. Missing edges are treated as no-op.
    pub fn delete_edge(&self, edge: &EdgeRecord) -> Result<(), StorageError> {
        let key = format!("{}|{}|{}", edge.from, edge.to, edge.kind);
        self.with_db(|db| {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(EDGES)?;
                let _ = table.remove(key.as_str())?;
            }
            write_txn.commit()?;
            Ok(())
        })
    }

    /// Returns all edges originating from `from`.
    pub fn edges_from(&self, from: &Uuid) -> Result<Vec<EdgeRecord>, StorageError> {
        let prefix = from.to_string();
        self.with_db(|db| {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(EDGES)?;
            let mut edges = Vec::new();
            for result in table.iter()? {
                let (key, _) = result?;
                let k = key.value();
                if k.starts_with(prefix.as_str()) {
                    if let Some(edge) = parse_edge_key(k) {
                        edges.push(edge);
                    }
                }
            }
            Ok(edges)
        })
    }

    /// Returns every edge in the store.
    pub fn list_all_edges(&self) -> Result<Vec<EdgeRecord>, StorageError> {
        self.with_db(|db| {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(EDGES)?;
            let mut edges = Vec::new();
            for result in table.iter()? {
                let (key, _) = result?;
                if let Some(edge) = parse_edge_key(key.value()) {
                    edges.push(edge);
                }
            }
            Ok(edges)
        })
    }

    // ── scan root registry ───────────────────────────────────────────────────

    pub fn add_scan_root(&self, root: &ScanRoot) -> Result<(), StorageError> {
        let path_str = root.path.to_string_lossy().into_owned();
        let label = root.label.clone();
        self.with_db(|db| {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(SCAN_ROOTS_TABLE)?;
                table.insert(path_str.as_str(), label.as_str())?;
            }
            write_txn.commit()?;
            Ok(())
        })
    }

    pub fn list_scan_roots(&self) -> Result<Vec<ScanRoot>, StorageError> {
        self.with_db(|db| {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(SCAN_ROOTS_TABLE)?;
            let mut roots = Vec::new();
            for result in table.iter()? {
                let (key, value) = result?;
                roots.push(ScanRoot {
                    path: std::path::PathBuf::from(key.value()),
                    label: value.value().to_string(),
                });
            }
            Ok(roots)
        })
    }

    // ── lease CRUD ────────────────────────────────────────────────────────────

    pub fn insert_lease(&self, lease: &LeaseInfo) -> Result<(), StorageError> {
        let bytes = bincode::serialize(lease)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        let key = lease.ticket_id.to_string();
        self.with_db(|db| {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(LEASES)?;
                table.insert(key.as_str(), bytes.as_slice())?;
            }
            write_txn.commit()?;
            Ok(())
        })
    }

    pub fn get_lease(&self, ticket_id: &Uuid) -> Result<Option<LeaseInfo>, StorageError> {
        let key = ticket_id.to_string();
        self.with_db(|db| {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(LEASES)?;
            match table.get(key.as_str())? {
                Some(value) => {
                    let lease: LeaseInfo = bincode::deserialize(value.value())
                        .map_err(|e| StorageError::Serialization(e.to_string()))?;
                    Ok(Some(lease))
                }
                None => Ok(None),
            }
        })
    }

    pub fn remove_lease(&self, ticket_id: &Uuid) -> Result<(), StorageError> {
        let key = ticket_id.to_string();
        self.with_db(|db| {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(LEASES)?;
                table.remove(key.as_str())?;
            }
            write_txn.commit()?;
            Ok(())
        })
    }

    pub fn list_active_leases(&self) -> Result<Vec<LeaseInfo>, StorageError> {
        self.with_db(|db| {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(LEASES)?;
            let mut leases = Vec::new();
            for result in table.iter()? {
                let (_, value) = result?;
                let lease: LeaseInfo = bincode::deserialize(value.value())
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                leases.push(lease);
            }
            Ok(leases)
        })
    }

    // ── internal ──────────────────────────────────────────────────────────────

    /// BFS reachability check: returns `true` if `target` is reachable from `start`
    /// following outgoing edges. Used for cycle detection.
    pub fn is_reachable(&self, start: &Uuid, target: &Uuid) -> Result<bool, StorageError> {
        use std::collections::{HashSet, VecDeque};
        self.with_db(|db| {
            let all_edges = {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(EDGES)?;
                let mut edges = Vec::new();
                for result in table.iter()? {
                    let (key, _) = result?;
                    if let Some(edge) = parse_edge_key(key.value()) {
                        edges.push(edge);
                    }
                }
                edges
            };

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
        })
    }
}

// ── free helpers ──────────────────────────────────────────────────────────────

fn open_db(path: &Path) -> Result<Database, StorageError> {
    if path.exists() {
        Database::open(path).map_err(|e| StorageError::Database(e.to_string()))
    } else {
        Database::create(path).map_err(|e| StorageError::Database(e.to_string()))
    }
}

fn ensure_tables(db: &Database) -> Result<(), StorageError> {
    use super::board::{BOARD_ACTIVE_INDEX, BOARD_CONFIG, BOARD_ENTRIES};
    let write_txn = db.begin_write()?;
    {
        write_txn.open_table(TICKETS)?;
        write_txn.open_table(EDGES)?;
        write_txn.open_table(SCAN_ROOTS_TABLE)?;
        write_txn.open_table(LEASES)?;
        write_txn.open_table(META)?;
        write_txn.open_table(BOARD_ENTRIES)?;
        write_txn.open_table(BOARD_ACTIVE_INDEX)?;
        write_txn.open_table(BOARD_CONFIG)?;
    }
    write_txn.commit()?;
    Ok(())
}

fn check_or_set_schema_version(db: &Database) -> Result<(), StorageError> {
    use crate::storage::schema::ensure_supported_schema_version;
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(META)?;
        let existing: Option<String> = table
            .get("schema_version")?
            .map(|v| v.value().to_string());
        match existing {
            Some(found) => {
                ensure_supported_schema_version(&found)?;
            }
            None => {
                table.insert("schema_version", SCHEMA_VERSION)?;
            }
        }
    }
    write_txn.commit()?;
    Ok(())
}

fn parse_edge_key(k: &str) -> Option<EdgeRecord> {
    let parts: Vec<&str> = k.splitn(3, '|').collect();
    if parts.len() != 3 {
        return None;
    }
    let from: Uuid = parts[0].parse().ok()?;
    let to: Uuid = parts[1].parse().ok()?;
    let kind = parts[2].to_string();
    Some(EdgeRecord {
        from,
        to,
        kind,
        created_at: chrono::Utc::now(),
    })
}
