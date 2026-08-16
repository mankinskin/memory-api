use std::time::UNIX_EPOCH;

use crate::{
    error::FsApiError,
    request::StatRequest,
    response::{
        EntryKind,
        StatResult,
    },
};

/// Get file or directory metadata without reading content.
pub fn stat(request: &StatRequest) -> Result<StatResult, FsApiError> {
    let path = &request.path;

    if !path.exists() {
        return Ok(StatResult {
            exists: false,
            kind: None,
            size: None,
            modified_secs: None,
        });
    }

    let metadata =
        path.metadata()
            .map_err(|e| FsApiError::CannotReadMetadata {
                path: path.to_path_buf(),
                source: e,
            })?;

    let kind = if metadata.is_dir() {
        EntryKind::Directory
    } else if metadata.is_symlink() {
        EntryKind::Symlink
    } else {
        EntryKind::File
    };

    let size = if kind == EntryKind::File {
        Some(metadata.len())
    } else {
        None
    };

    let modified_secs = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());

    Ok(StatResult {
        exists: true,
        kind: Some(kind),
        size,
        modified_secs,
    })
}
