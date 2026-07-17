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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{DebounceEventResult, DebouncedEvent, new_debouncer};

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

/// Watch `project_root` for filesystem changes and run an incremental
/// `sync` whenever a debounced batch of relevant events arrives.
///
/// `debounce_ms` is the quiet period after the last event before `sync`
/// is triggered. The loop exits when `stop` is set, returning `Ok(())`.
/// Changes inside `.tooned/` and `.git/` are ignored to avoid feedback
/// loops, and the project `.gitignore` is respected where possible.
pub fn watch_with_stop(
    project_root: &Path,
    debounce_ms: u64,
    stop: &AtomicBool,
) -> Result<(), IndexError> {
    if !schema::index_exists(project_root) {
        return Err(IndexError::NoIndex(project_root.to_path_buf()));
    }

    let (tx, rx) = channel::<DebounceEventResult>();
    let debounce = Duration::from_millis(debounce_ms);
    let mut debouncer = new_debouncer(debounce, move |res: DebounceEventResult| {
        // The watcher runs on its own thread; if the receiver has gone
        // away the process is shutting down and the error can be ignored.
        let _ = tx.send(res);
    })?;
    debouncer.watcher().watch(project_root, RecursiveMode::Recursive)?;

    let filter = build_gitignore_filter(project_root).unwrap_or_else(|_| {
        // If we cannot read the gitignore file, still fall back to the
        // hard-coded ignores so `.tooned/` updates don't self-trigger.
        ignore::gitignore::Gitignore::empty()
    });

    let mut count: u64 = 0;
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }

        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(events)) => {
                let relevant = events.iter().any(|event| !is_ignored(event, project_root, &filter));
                if !relevant {
                    continue;
                }
                count += 1;
                match sync(project_root) {
                    Ok(summary) => println!(
                        "[watch #{count}] synced {}: +{} ~{} -{} ({} unchanged)",
                        project_root.display(),
                        summary.added,
                        summary.updated,
                        summary.removed,
                        summary.unchanged
                    ),
                    Err(err) => eprintln!("tooned index watch: sync failed: {err}"),
                }
            }
            Ok(Err(err)) => {
                eprintln!("tooned index watch: watcher error: {err}");
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}

/// Watch `project_root` until the process is interrupted. Backs the
/// `tooned index watch` CLI; for tests or other callers that need to
/// stop the loop, use [`watch_with_stop`].
pub fn watch(project_root: &Path, debounce_ms: u64) -> Result<(), IndexError> {
    watch_with_stop(project_root, debounce_ms, &AtomicBool::new(false))
}

fn build_gitignore_filter(
    project_root: &Path,
) -> Result<ignore::gitignore::Gitignore, ignore::Error> {
    let mut builder = ignore::gitignore::GitignoreBuilder::new(project_root);
    let gitignore = project_root.join(".gitignore");
    if gitignore.is_file() {
        builder.add(gitignore);
    }
    builder.add_line(None, ".tooned/")?;
    builder.add_line(None, ".git/")?;
    builder.build()
}

fn is_ignored(
    event: &DebouncedEvent,
    project_root: &Path,
    filter: &ignore::gitignore::Gitignore,
) -> bool {
    let Ok(rel) = event.path.strip_prefix(project_root) else {
        return true;
    };
    let rel_str = rel.to_string_lossy();
    if rel_str == ".tooned" || rel_str.starts_with(".tooned/") {
        return true;
    }
    if rel_str == ".git" || rel_str.starts_with(".git/") {
        return true;
    }
    let is_dir = std::fs::metadata(&event.path).is_ok_and(|m| m.is_dir());
    filter.matched(&*rel_str, is_dir).is_ignore()
}

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("filesystem watcher error: {0}")]
    Watcher(#[from] notify::Error),
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
