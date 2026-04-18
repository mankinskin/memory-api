use crate::error::StorageSchemaError;

pub const SCHEMA_VERSION: &str = "0";

pub const TABLE_TICKETS: &str = "tickets";
pub const TABLE_EDGES: &str = "edges";
pub const TABLE_SCAN_ROOTS: &str = "scan_roots";
pub const TABLE_LEASES: &str = "leases";
pub const TABLE_META: &str = "meta";

pub const REQUIRED_TABLES: [&str; 5] = [
    TABLE_TICKETS,
    TABLE_EDGES,
    TABLE_SCAN_ROOTS,
    TABLE_LEASES,
    TABLE_META,
];

pub fn ensure_supported_schema_version(found: &str) -> Result<(), StorageSchemaError> {
    if found == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(StorageSchemaError::VersionMismatch {
            found: found.to_string(),
            expected: SCHEMA_VERSION.to_string(),
        })
    }
}
