use std::collections::{
    HashSet,
    VecDeque,
};

use rusqlite::params;
use uuid::Uuid;

use crate::{
    error::StorageError,
    model::filesystem::ScanRoot,
};

use super::RedbIndexStore;
use crate::storage::{
    indexed::LeaseInfo,
    schema::{
        TABLE_EDGES,
        TABLE_LEASES,
        TABLE_SCAN_ROOTS,
        TABLE_TICKETS,
    },
};

impl RedbIndexStore {
    /// Returns the number of indexed tickets without deserializing rows.
    pub fn count_tickets(&self) -> Result<usize, StorageError> {
        let conn = self.read_conn()?;
        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {TABLE_TICKETS}"),
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Returns the number of edges without fetching the full edge list.
    pub fn count_edges(&self) -> Result<usize, StorageError> {
        let conn = self.read_conn()?;
        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {TABLE_EDGES}"),
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn add_scan_root(
        &self,
        root: &ScanRoot,
    ) -> Result<(), StorageError> {
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
        let mut stmt = conn
            .prepare(&format!("SELECT path, label FROM {TABLE_SCAN_ROOTS}"))?;
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

    pub fn insert_lease(
        &self,
        lease: &LeaseInfo,
    ) -> Result<(), StorageError> {
        let bytes = bincode::serialize(lease)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
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

    pub fn get_lease(
        &self,
        ticket_id: &Uuid,
    ) -> Result<Option<LeaseInfo>, StorageError> {
        let key = ticket_id.to_string();
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT data FROM {TABLE_LEASES} WHERE id = ?1"
        ))?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let bytes: Vec<u8> = row.get(0)?;
            let lease: LeaseInfo =
                bincode::deserialize(&bytes).map_err(|error| {
                    StorageError::Serialization(error.to_string())
                })?;
            Ok(Some(lease))
        } else {
            Ok(None)
        }
    }

    pub fn remove_lease(
        &self,
        ticket_id: &Uuid,
    ) -> Result<(), StorageError> {
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
        let mut stmt =
            conn.prepare(&format!("SELECT data FROM {TABLE_LEASES}"))?;
        let rows = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        let mut leases = Vec::new();
        for bytes in rows {
            let lease: LeaseInfo =
                bincode::deserialize(&bytes?).map_err(|error| {
                    StorageError::Serialization(error.to_string())
                })?;
            leases.push(lease);
        }
        Ok(leases)
    }

    /// BFS reachability check: returns `true` if `target` is reachable from
    /// `start` following outgoing edges. Used for cycle detection.
    pub fn is_reachable(
        &self,
        start: &Uuid,
        target: &Uuid,
    ) -> Result<bool, StorageError> {
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
            for edge in all_edges.iter().filter(|edge| edge.from == current) {
                queue.push_back(edge.to);
            }
        }
        Ok(false)
    }
}
