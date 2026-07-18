// SPDX-License-Identifier: AGPL-3.0-only

//! Incremental sync (T057): stat-first logic (check `mtime` before
//! re-hashing), prune rows for files that no longer exist.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;
use tooned_core::ConversionOptions;

use crate::IndexError;
use crate::filter::IndexFilter;
use crate::scan::{
    build_walker, enforce_walk_cap, hash_file_streaming, is_tooned_internal,
    persist_oversized_file, persist_scanned_file,
};
use crate::schema::{self, file_mtime_unix, now_unix};

/// Result of a [`sync`] run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncSummary {
    /// Newly discovered files.
    pub added: usize,
    /// Files whose content actually changed (re-hashed, re-classified).
    pub updated: usize,
    /// Files whose `mtime` was unchanged (skipped entirely) or whose
    /// `mtime` changed but content did not (touch-without-edit).
    pub unchanged: usize,
    /// Previously indexed files no longer present under the scanned root.
    pub removed: usize,
}

/// Incremental sync of an existing index: for every file currently under
/// `project_root`, skips re-hashing entirely when `mtime` is unchanged
/// since the last scan; re-hashes (but only re-classifies if the content
/// hash actually changed) when `mtime` differs; adds newly discovered
/// files; and prunes rows (cascading to `shapes`/`conversions`) for files
/// that no longer exist. Requires a prior [`crate::scan_full`] --
/// `Err(IndexError::NoIndex)` if no index exists yet.
pub fn sync(project_root: &Path, filter: &IndexFilter) -> Result<SyncSummary, IndexError> {
    if !schema::index_exists(project_root) {
        return Err(IndexError::NoIndex(project_root.to_path_buf()));
    }
    let mut conn = schema::open_index(project_root)?;

    let existing = load_existing(&conn, filter)?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut summary = SyncSummary::default();

    // See `scan_full`'s equivalent comment: one transaction for the whole
    // sync, not one implicit auto-commit transaction per touched row.
    let tx = conn.transaction()?;

    let exclude_gitignore = filter.compile_excludes(project_root).unwrap_or_else(|_| {
        // If exclude compilation fails, fall back to an empty gitignore
        // (no exclusion) rather than failing the entire sync.
        ignore::gitignore::Gitignore::empty()
    });

    let walker = build_walker(project_root);

    let mut visited: usize = 0;
    for entry in walker {
        visited += 1;
        enforce_walk_cap(visited)?;

        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                // A yielded entry we can't read is transient; record it as seen so
                // the prune pass does not delete a row for a file that still exists.
                if let ignore::Error::WithPath { path, .. } = err {
                    let key = path.strip_prefix(project_root).map_or_else(
                        |_| path.to_string_lossy().into_owned(),
                        |r| r.to_string_lossy().into_owned(),
                    );
                    seen.insert(key);
                }
                continue;
            }
        };
        let Some(file_type) = entry.file_type() else {
            // No file type (rare); treat as seen so the prune pass does not
            // delete a row for a file that still exists on disk.
            let key = entry.path().strip_prefix(project_root).map_or_else(
                |_| entry.path().to_string_lossy().into_owned(),
                |r| r.to_string_lossy().into_owned(),
            );
            seen.insert(key);
            continue;
        };
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let Ok(rel) = path.strip_prefix(project_root) else { continue };
        let Some(rel_str) = rel.to_str() else {
            // Non-UTF8 path: still present on disk, so record it as seen to stop
            // the prune pass from deleting a legitimately-present file's row
            // (finding: sync prune could drop present files with non-UTF8 names).
            seen.insert(entry.path().to_string_lossy().into_owned());
            continue;
        };
        if is_tooned_internal(rel_str) {
            // tooned's own internal files are never indexed; mark seen so they
            // are not pruned from a prior (stale) scan.
            seen.insert(rel_str.to_string());
            continue;
        }

        // Check if file is excluded - if so, mark as seen but skip processing
        if !filter.excludes.is_empty() && filter.is_excluded(path, project_root, &exclude_gitignore)
        {
            seen.insert(rel_str.to_string());
            continue;
        }

        let meta = match std::fs::metadata(path) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                // Transient metadata failure (e.g. permission denied) must not
                // be interpreted as "file deleted" during the prune pass.
                seen.insert(rel_str.to_string());
                continue;
            }
        };
        let mtime = file_mtime_unix(&meta);
        let size = crate::schema::saturating_i64(meta.len());
        seen.insert(rel_str.to_string());

        // Mirrors `scan_full`'s size check: never `std::fs::read` a whole
        // file into memory before knowing it's within `max_input_bytes`.
        // Content-changed detection still needs a real hash for files above
        // the cap, but it's computed via a streaming reader that never
        // buffers the whole file at once.
        let oversized = meta.len() > ConversionOptions::default().max_input_bytes as u64;

        match existing.get(rel_str) {
            Some((old_mtime, old_size, _)) if *old_mtime == mtime && *old_size == size => {
                // Stat-first: mtime and size both unchanged since the last
                // scan, skip re-hashing (and any DB write at all) entirely.
                summary.unchanged += 1;
            }
            Some((_, _, old_hash)) => {
                if oversized {
                    let doc_type = crate::scan::detect_file_type(path, filter);
                    if !filter.matches_type(doc_type) {
                        // File no longer matches type filter; skip but mark as seen
                        // so it's not pruned (it may match a future filter).
                        continue;
                    }
                    let Ok(new_hash) = hash_file_streaming(path) else { continue };
                    if &new_hash == old_hash {
                        touch_mtime(&tx, rel_str, mtime)?;
                        summary.unchanged += 1;
                    } else {
                        persist_oversized_file(&tx, rel_str, &new_hash, meta.len(), mtime)?;
                        summary.updated += 1;
                    }
                    continue;
                }
                let Ok(bytes) = std::fs::read(path) else { continue };
                let doc_type = tooned_core::inspect(&bytes, &ConversionOptions::default()).doc_type;
                if !filter.matches_type(doc_type) {
                    // File no longer matches type filter; skip but mark as seen
                    // so it's not pruned (it may match a future filter).
                    continue;
                }
                let new_hash = blake3::hash(&bytes).to_hex().to_string();
                if &new_hash == old_hash {
                    // mtime changed but content didn't (e.g. `touch`):
                    // update mtime only, skip re-classification.
                    touch_mtime(&tx, rel_str, mtime)?;
                    summary.unchanged += 1;
                } else {
                    persist_scanned_file(&tx, rel_str, &bytes, meta.len(), mtime)?;
                    summary.updated += 1;
                }
            }
            None => {
                if oversized {
                    let doc_type = crate::scan::detect_file_type(path, filter);
                    if !filter.matches_type(doc_type) {
                        continue;
                    }
                    let Ok(new_hash) = hash_file_streaming(path) else { continue };
                    persist_oversized_file(&tx, rel_str, &new_hash, meta.len(), mtime)?;
                    summary.added += 1;
                    continue;
                }
                let Ok(bytes) = std::fs::read(path) else { continue };
                let doc_type = tooned_core::inspect(&bytes, &ConversionOptions::default()).doc_type;
                if !filter.matches_type(doc_type) {
                    continue;
                }
                persist_scanned_file(&tx, rel_str, &bytes, meta.len(), mtime)?;
                summary.added += 1;
            }
        }
    }

    for path in existing.keys() {
        if !seen.contains(path) {
            delete_file(&tx, path)?;
            summary.removed += 1;
        }
    }

    tx.commit()?;

    Ok(summary)
}

fn load_existing(
    conn: &Connection,
    _filter: &IndexFilter,
) -> Result<HashMap<String, (i64, i64, String)>, IndexError> {
    let mut stmt = conn.prepare("SELECT path, mtime_unix, size_bytes, content_hash FROM files")?;
    let rows = stmt.query_map([], |row| {
        let path: String = row.get(0)?;
        let mtime: i64 = row.get(1)?;
        let size: i64 = row.get(2)?;
        let hash: String = row.get(3)?;
        Ok((path, mtime, size, hash))
    })?;

    let mut map = HashMap::new();
    for row in rows {
        let (path, mtime, size, hash) = row?;
        // Load all files regardless of type filter, so the prune pass can correctly
        // identify files that are truly deleted vs. files that are just excluded
        // or don't match the current type filter.
        map.insert(path, (mtime, size, hash));
    }
    Ok(map)
}

fn touch_mtime(conn: &Connection, path: &str, mtime: i64) -> Result<(), IndexError> {
    conn.execute(
        "UPDATE files SET mtime_unix = ?1, scanned_at = ?2 WHERE path = ?3",
        rusqlite::params![mtime, now_unix(), path],
    )?;
    Ok(())
}

fn delete_file(conn: &Connection, path: &str) -> Result<(), IndexError> {
    conn.execute("DELETE FROM files WHERE path = ?1", [path])?;
    Ok(())
}
