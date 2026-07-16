// SPDX-License-Identifier: AGPL-3.0-only

//! # tooned-index
//!
//! The `.tooned/` on-disk SQLite index: directory scanning, content
//! fingerprinting, and cached shape/conversion reports, invoked on-demand by
//! `tooned index` / `tooned index sync` / `tooned stats` — never on the
//! hot hook path (see `tooned-core` for that).

mod gitignore;
mod scan;
mod schema;
mod sync;

use std::path::{Path, PathBuf};

pub use scan::{ScanSummary, scan_full};
pub use schema::{ConversionRecord, FileRecord, ShapeRecord, index_db_path, index_exists};
pub use sync::{SyncSummary, sync};

/// Checkpoint the SQLite WAL and truncate the `-wal` file (backs
/// `tooned index compact`). Safe to call on a live index; concurrent readers
/// are not blocked.
pub fn compact(project_root: &Path) -> Result<(), IndexError> {
    if !schema::index_exists(project_root) {
        return Err(IndexError::NoIndex(project_root.to_path_buf()));
    }
    let conn = schema::open_index(project_root)?;
    conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")?;
    Ok(())
}

/// Poll `tooned index sync` every `interval_secs` seconds.
///
/// This is a minimal, dependency-light watch implementation. A future
/// iteration should replace the polling loop with a `notify`-based
/// filesystem watcher with debounce.
pub fn watch(root: &Path, interval_secs: u64) -> Result<(), IndexError> {
    if !schema::index_exists(root) {
        return Err(IndexError::NoIndex(root.to_path_buf()));
    }
    let interval = std::time::Duration::from_secs(interval_secs);
    let mut count: u64 = 0;
    loop {
        count += 1;
        match sync(root) {
            Ok(summary) => println!(
                "[watch #{count}] synced {}: +{} ~{} -{} ({} unchanged)",
                root.display(),
                summary.added,
                summary.updated,
                summary.removed,
                summary.unchanged
            ),
            Err(err) => eprintln!("tooned index watch: sync failed: {err}"),
        }
        std::thread::sleep(interval);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("no index found at {0}; run `tooned index` first")]
    NoIndex(PathBuf),
    #[error("file not indexed: {0}")]
    FileNotIndexed(String),
    #[error(
        "directory walk exceeds the safety limit of {0} entries; refusing to continue scanning \
         (this guards against pointing the scanner at an unexpectedly large directory tree, e.g. \
         a home directory rather than a project root)"
    )]
    ScanTooLarge(usize),
    #[error("unsupported schema version: {0}; delete or migrate the index database")]
    UnsupportedSchemaVersion(String),
}

/// `tooned index status` report: whether an index exists, how many files
/// it tracks, and when it was last scanned. Reporting "no index yet" is a
/// normal value here, not an error -- per contract, `index status` always
/// exits 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStatus {
    pub exists: bool,
    /// `SELECT COUNT(*)` is always non-negative; kept as `i64` (SQLite's
    /// native integer width) rather than converted to `usize`, so no
    /// conversion/fallback is needed at all.
    pub file_count: i64,
    pub last_scanned_at: Option<i64>,
}

/// Reports index existence/size/freshness without requiring the caller to
/// already know whether an index exists (backs `tooned index status`).
pub fn status(project_root: &Path) -> Result<IndexStatus, IndexError> {
    if !schema::index_exists(project_root) {
        return Ok(IndexStatus { exists: false, file_count: 0, last_scanned_at: None });
    }
    let conn = schema::open_index(project_root)?;
    let file_count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
    let last_scanned_at: Option<i64> =
        conn.query_row("SELECT MAX(scanned_at) FROM files", [], |row| row.get(0))?;
    Ok(IndexStatus { exists: true, file_count, last_scanned_at })
}

/// The full indexed record for one file (backs `tooned index show`).
#[derive(Debug, Clone, PartialEq)]
pub struct FileDetail {
    pub file: FileRecord,
    pub shapes: Vec<ShapeRecord>,
    pub conversions: Vec<ConversionRecord>,
}

/// Looks up one file's indexed record by its project-relative path.
///
/// # Errors
/// `IndexError::NoIndex` if no index exists yet for `project_root`;
/// `IndexError::FileNotIndexed` if the index exists but has no row for
/// `rel_path`. Neither case panics -- both resolve to a typed, reportable
/// error (backs `tooned index show`'s graceful "not indexed" behavior).
pub fn show_file(project_root: &Path, rel_path: &str) -> Result<FileDetail, IndexError> {
    if !schema::index_exists(project_root) {
        return Err(IndexError::NoIndex(project_root.to_path_buf()));
    }
    let conn = schema::open_index(project_root)?;

    let file = conn
        .query_row(
            "SELECT path, size_bytes, mtime_unix, content_hash, doc_type, scanned_at \
             FROM files WHERE path = ?1",
            [rel_path],
            |row| {
                Ok(FileRecord {
                    path: row.get(0)?,
                    size_bytes: row.get(1)?,
                    mtime_unix: row.get(2)?,
                    content_hash: row.get(3)?,
                    doc_type: row.get(4)?,
                    scanned_at: row.get(5)?,
                })
            },
        )
        .map_err(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => {
                IndexError::FileNotIndexed(rel_path.to_string())
            }
            other => IndexError::Sqlite(other),
        })?;

    let mut shape_stmt = conn.prepare(
        "SELECT path, json_pointer, shape_class, uniformity_pct, sampled_count \
         FROM shapes WHERE path = ?1",
    )?;
    let shapes = shape_stmt
        .query_map([rel_path], |row| {
            Ok(ShapeRecord {
                path: row.get(0)?,
                json_pointer: row.get(1)?,
                shape_class: row.get(2)?,
                uniformity_pct: row.get(3)?,
                sampled_count: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut conv_stmt = conn.prepare(
        "SELECT path, json_pointer, json_bytes, toon_bytes, savings_pct, cached_toon_text, computed_at \
         FROM conversions WHERE path = ?1",
    )?;
    let conversions = conv_stmt
        .query_map([rel_path], |row| {
            Ok(ConversionRecord {
                path: row.get(0)?,
                json_pointer: row.get(1)?,
                json_bytes: row.get(2)?,
                toon_bytes: row.get(3)?,
                savings_pct: row.get(4)?,
                cached_toon_text: row.get(5)?,
                computed_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(FileDetail { file, shapes, conversions })
}

/// Ranked savings report: `conversions` ordered by `savings_pct` descending,
/// limited to `top` entries (all rows if `None`) (backs `tooned stats`).
///
/// # Errors
/// `IndexError::NoIndex` if no index exists yet for `project_root`.
pub fn stats(project_root: &Path, top: Option<u32>) -> Result<Vec<ConversionRecord>, IndexError> {
    if !schema::index_exists(project_root) {
        return Err(IndexError::NoIndex(project_root.to_path_buf()));
    }
    let conn = schema::open_index(project_root)?;
    let limit: i64 = match top {
        Some(n) => i64::from(n),
        None => -1, // SQLite: LIMIT -1 means "no limit".
    };
    let mut stmt = conn.prepare(
        "SELECT path, json_pointer, json_bytes, toon_bytes, savings_pct, cached_toon_text, computed_at \
         FROM conversions \
         ORDER BY savings_pct DESC \
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit], |row| {
            Ok(ConversionRecord {
                path: row.get(0)?,
                json_pointer: row.get(1)?,
                json_bytes: row.get(2)?,
                toon_bytes: row.get(3)?,
                savings_pct: row.get(4)?,
                cached_toon_text: row.get(5)?,
                computed_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
