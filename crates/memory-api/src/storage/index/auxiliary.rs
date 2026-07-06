use std::collections::{
    HashSet,
    VecDeque,
};

use rusqlite::params;
use uuid::Uuid;

use crate::{
    error::StorageError,
    model::filesystem::{
        PersistedScanRoot,
        PolicyDecision,
        ScanRoot,
        ScanRootMetadata,
        ScanRootSource,
    },
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
        self.add_scan_root_with_metadata(root, &ScanRootMetadata::default())
    }

    pub fn add_scan_root_with_metadata(
        &self,
        root: &ScanRoot,
        metadata: &ScanRootMetadata,
    ) -> Result<(), StorageError> {
        let path_str = root.path.to_string_lossy().into_owned();
        let label = root.label.clone();
        let source = metadata.source.as_str();
        let policy_decision = metadata.policy_decision.as_str();
        let workspace_root = metadata
            .workspace_root
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        self.with_write(|conn| {
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_SCAN_ROOTS} (path, label, source, policy_decision, workspace_root) VALUES (?1, ?2, ?3, ?4, ?5)"
                ),
                params![path_str, label, source, policy_decision, workspace_root],
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

    pub fn list_scan_roots_with_metadata(
        &self
    ) -> Result<Vec<PersistedScanRoot>, StorageError> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT path, label, source, policy_decision, workspace_root FROM {TABLE_SCAN_ROOTS}"
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut roots = Vec::new();
        for row in rows {
            let (path_str, label, source, policy_decision, workspace_root) = row?;
            roots.push(PersistedScanRoot {
                root: ScanRoot {
                    path: std::path::PathBuf::from(path_str),
                    label,
                },
                metadata: ScanRootMetadata {
                    source: ScanRootSource::from_str_or_default(&source),
                    policy_decision: PolicyDecision::from_str_or_default(
                        &policy_decision,
                    ),
                    workspace_root: workspace_root
                        .map(std::path::PathBuf::from),
                },
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rusqlite::Connection;
    use tempfile::tempdir;

    use super::*;
    use crate::model::filesystem::{
        PolicyDecision,
        ScanRootSource,
    };
    use crate::storage::index::RedbIndexStore;

    fn open_store(dir: &std::path::Path) -> RedbIndexStore {
        RedbIndexStore::open(&dir.join("index.sqlite")).unwrap()
    }

    #[test]
    fn add_scan_root_defaults_metadata() {
        let dir = tempdir().unwrap();
        let store = open_store(dir.path());
        store
            .add_scan_root(&ScanRoot {
                path: PathBuf::from("/ws/.ticket/tickets"),
                label: ".".to_string(),
            })
            .unwrap();

        let roots = store.list_scan_roots_with_metadata().unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].metadata.source, ScanRootSource::Discovered);
        assert_eq!(roots[0].metadata.policy_decision, PolicyDecision::Included);
        assert_eq!(roots[0].metadata.workspace_root, None);
    }

    #[test]
    fn scan_root_metadata_round_trips() {
        let dir = tempdir().unwrap();
        let store = open_store(dir.path());
        store
            .add_scan_root_with_metadata(
                &ScanRoot {
                    path: PathBuf::from("/ws/fixtures/.ticket/tickets"),
                    label: "fixtures".to_string(),
                },
                &ScanRootMetadata {
                    source: ScanRootSource::Policy,
                    policy_decision: PolicyDecision::Ignored,
                    workspace_root: Some(PathBuf::from("/ws/fixtures")),
                },
            )
            .unwrap();

        let roots = store.list_scan_roots_with_metadata().unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].metadata.source, ScanRootSource::Policy);
        assert_eq!(roots[0].metadata.policy_decision, PolicyDecision::Ignored);
        assert_eq!(
            roots[0].metadata.workspace_root,
            Some(PathBuf::from("/ws/fixtures"))
        );
    }

    #[test]
    fn migration_backfills_legacy_scan_roots_table() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("index.sqlite");

        // Simulate a pre-metadata index with only (path, label).
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(&format!(
                "CREATE TABLE {TABLE_SCAN_ROOTS} (
                     path  TEXT PRIMARY KEY NOT NULL,
                     label TEXT NOT NULL
                 );
                 INSERT INTO {TABLE_SCAN_ROOTS} (path, label)
                 VALUES ('/legacy/.ticket/tickets', 'legacy');"
            ))
            .unwrap();
        }

        // Opening the store runs the non-destructive migration.
        let store = RedbIndexStore::open(&db_path).unwrap();
        let roots = store.list_scan_roots_with_metadata().unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].root.label, "legacy");
        assert_eq!(roots[0].metadata.source, ScanRootSource::Discovered);
        assert_eq!(roots[0].metadata.policy_decision, PolicyDecision::Included);
        assert_eq!(roots[0].metadata.workspace_root, None);
    }
}
