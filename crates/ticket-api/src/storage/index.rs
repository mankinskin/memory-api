use std::path::Path;

use redb::{Database, ReadableTable, TableDefinition};
use uuid::Uuid;

use crate::error::StorageError;
use crate::model::edge::EdgeRecord;
use crate::model::filesystem::ScanRoot;
use crate::storage::schema::{SCHEMA_VERSION, TABLE_EDGES, TABLE_LEASES, TABLE_META, TABLE_SCAN_ROOTS, TABLE_TICKETS};

use super::indexed::{IndexedTicket, LeaseInfo};

const TICKETS: TableDefinition<&str, &[u8]> = TableDefinition::new(TABLE_TICKETS);
const EDGES: TableDefinition<&str, ()> = TableDefinition::new(TABLE_EDGES);
const SCAN_ROOTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new(TABLE_SCAN_ROOTS);
const LEASES: TableDefinition<&str, &[u8]> = TableDefinition::new(TABLE_LEASES);
const META: TableDefinition<&str, &str> = TableDefinition::new(TABLE_META);

pub struct RedbIndexStore {
    db: Database,
}

impl RedbIndexStore {
    pub fn open(db_path: &Path) -> Result<Self, StorageError> {
        let db = if db_path.exists() {
            Database::open(db_path).map_err(|e| StorageError::Database(e.to_string()))?
        } else {
            Database::create(db_path).map_err(|e| StorageError::Database(e.to_string()))?
        };
        let store = Self { db };
        store.ensure_tables()?;
        store.check_or_set_schema_version()?;
        Ok(store)
    }

    fn ensure_tables(&self) -> Result<(), StorageError> {
        let write_txn = self.db.begin_write()?;
        {
            write_txn.open_table(TICKETS)?;
            write_txn.open_table(EDGES)?;
            write_txn.open_table(SCAN_ROOTS_TABLE)?;
            write_txn.open_table(LEASES)?;
            write_txn.open_table(META)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn check_or_set_schema_version(&self) -> Result<(), StorageError> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(META)?;
            // Eagerly copy the value out so the immutable borrow from get() is dropped
            // before we potentially call insert().
            let existing: Option<String> = table
                .get("schema_version")?
                .map(|v| v.value().to_string());
            match existing {
                Some(v) => {
                    crate::storage::schema::ensure_supported_schema_version(&v)?;
                }
                None => {
                    table.insert("schema_version", SCHEMA_VERSION)?;
                }
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    // ── ticket CRUD ──────────────────────────────────────────────────────────

    pub fn insert_ticket(&self, ticket: &IndexedTicket) -> Result<(), StorageError> {
        let bytes = bincode::serialize(ticket)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        let key = ticket.id.to_string();
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TICKETS)?;
            table.insert(key.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_ticket(&self, id: &Uuid) -> Result<Option<IndexedTicket>, StorageError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TICKETS)?;
        let key = id.to_string();
        match table.get(key.as_str())? {
            Some(value) => {
                let ticket: IndexedTicket = bincode::deserialize(value.value())
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                Ok(Some(ticket))
            }
            None => Ok(None),
        }
    }

    pub fn list_tickets(&self, include_deleted: bool) -> Result<Vec<IndexedTicket>, StorageError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TICKETS)?;
        let mut tickets = Vec::new();
        for result in table.iter()? {
            let (_, value) = result?;
            let ticket: IndexedTicket = bincode::deserialize(value.value())
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            if include_deleted || !ticket.deleted {
                tickets.push(ticket);
            }
        }
        Ok(tickets)
    }

    /// Soft-delete: marks the index entry as deleted. Filesystem folder is not touched here.
    pub fn soft_delete_ticket(&self, id: &Uuid) -> Result<(), StorageError> {
        let mut ticket = self
            .get_ticket(id)?
            .ok_or(StorageError::NotFound(*id))?;
        ticket.deleted = true;
        ticket.updated_at = chrono::Utc::now();
        self.insert_ticket(&ticket)
    }

    // ── edge CRUD ─────────────────────────────────────────────────────────────

    /// Insert an edge using `"{from}|{to}|{kind}"` as the composite key.
    /// Duplicate insert is idempotent.
    pub fn insert_edge(&self, edge: &EdgeRecord) -> Result<(), StorageError> {
        let key = format!("{}|{}|{}", edge.from, edge.to, edge.kind);
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(EDGES)?;
            table.insert(key.as_str(), ())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Delete an edge by composite key. Missing edges are treated as no-op.
    pub fn delete_edge(&self, edge: &EdgeRecord) -> Result<(), StorageError> {
        let key = format!("{}|{}|{}", edge.from, edge.to, edge.kind);
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(EDGES)?;
            let _ = table.remove(key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Returns all edges originating from `from`.
    pub fn edges_from(&self, from: &Uuid) -> Result<Vec<EdgeRecord>, StorageError> {
        let prefix = from.to_string();
        let read_txn = self.db.begin_read()?;
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
    }

    /// Returns every edge in the store.
    pub fn list_all_edges(&self) -> Result<Vec<EdgeRecord>, StorageError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(EDGES)?;
        let mut edges = Vec::new();
        for result in table.iter()? {
            let (key, _) = result?;
            if let Some(edge) = parse_edge_key(key.value()) {
                edges.push(edge);
            }
        }
        Ok(edges)
    }

    // ── scan root registry ───────────────────────────────────────────────────

    pub fn add_scan_root(&self, root: &ScanRoot) -> Result<(), StorageError> {
        let path_str = root.path.to_string_lossy().into_owned();
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SCAN_ROOTS_TABLE)?;
            table.insert(path_str.as_str(), root.label.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn list_scan_roots(&self) -> Result<Vec<ScanRoot>, StorageError> {
        let read_txn = self.db.begin_read()?;
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
    }

    // ── lease CRUD ────────────────────────────────────────────────────────────

    pub fn insert_lease(&self, lease: &LeaseInfo) -> Result<(), StorageError> {
        let bytes = bincode::serialize(lease)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        let key = lease.ticket_id.to_string();
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(LEASES)?;
            table.insert(key.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_lease(&self, ticket_id: &Uuid) -> Result<Option<LeaseInfo>, StorageError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LEASES)?;
        let key = ticket_id.to_string();
        match table.get(key.as_str())? {
            Some(value) => {
                let lease: LeaseInfo = bincode::deserialize(value.value())
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                Ok(Some(lease))
            }
            None => Ok(None),
        }
    }

    pub fn remove_lease(&self, ticket_id: &Uuid) -> Result<(), StorageError> {
        let key = ticket_id.to_string();
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(LEASES)?;
            table.remove(key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn list_active_leases(&self) -> Result<Vec<LeaseInfo>, StorageError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LEASES)?;
        let mut leases = Vec::new();
        for result in table.iter()? {
            let (_, value) = result?;
            let lease: LeaseInfo = bincode::deserialize(value.value())
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            leases.push(lease);
        }
        Ok(leases)
    }

    // ── internal ──────────────────────────────────────────────────────────────

    /// BFS reachability check: returns `true` if `target` is reachable from `start`
    /// following outgoing edges. Used for cycle detection before inserting a new edge.
    pub fn is_reachable(&self, start: &Uuid, target: &Uuid) -> Result<bool, StorageError> {
        use std::collections::{HashSet, VecDeque};
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
            for edge in self.edges_from(&current)? {
                queue.push_back(edge.to);
            }
        }
        Ok(false)
    }
}

fn parse_edge_key(key: &str) -> Option<EdgeRecord> {
    let parts: Vec<&str> = key.splitn(3, '|').collect();
    if parts.len() != 3 {
        return None;
    }
    let from = parts[0].parse().ok()?;
    let to = parts[1].parse().ok()?;
    Some(EdgeRecord {
        from,
        to,
        kind: parts[2].to_string(),
        created_at: chrono::Utc::now(), // not persisted in edge key; placeholder
    })
}
