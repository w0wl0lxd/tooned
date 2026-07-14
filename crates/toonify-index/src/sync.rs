//! Incremental sync (T057): stat-first logic (check `mtime` before
//! re-hashing), prune rows for files that no longer exist.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;
use tooned_core::ConversionOptions;

use crate::IndexError;
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
pub fn sync(project_root: &Path) -> Result<SyncSummary, IndexError> {
    if !schema::index_exists(project_root) {
        return Err(IndexError::NoIndex(project_root.to_path_buf()));
    }
    let mut conn = schema::open_index(project_root)?;

    let existing = load_existing(&conn)?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut summary = SyncSummary::default();

    // See `scan_full`'s equivalent comment: one transaction for the whole
    // sync, not one implicit auto-commit transaction per touched row.
    let tx = conn.transaction()?;

    let mut visited: usize = 0;
    for entry in build_walker(project_root) {
        visited += 1;
        enforce_walk_cap(visited)?;

        let Ok(entry) = entry else { continue };
        let Some(file_type) = entry.file_type() else { continue };
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let Ok(rel) = path.strip_prefix(project_root) else { continue };
        let Some(rel_str) = rel.to_str() else { continue };
        if is_tooned_internal(rel_str) {
            continue;
        }

        let Ok(meta) = std::fs::metadata(path) else { continue };
        let mtime = file_mtime_unix(&meta);
        seen.insert(rel_str.to_string());

        // Mirrors `scan_full`'s size check: never `std::fs::read` a whole
        // file into memory before knowing it's within `max_input_bytes`.
        // Content-changed detection still needs a real hash for files above
        // the cap, but it's computed via a streaming reader that never
        // buffers the whole file at once.
        let oversized = meta.len() > ConversionOptions::default().max_input_bytes as u64;

        match existing.get(rel_str) {
            Some((old_mtime, _)) if *old_mtime == mtime => {
                // Stat-first: mtime unchanged since the last scan, skip
                // re-hashing (and any DB write at all) entirely.
                summary.unchanged += 1;
            }
            Some((_, old_hash)) => {
                if oversized {
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
                    let Ok(new_hash) = hash_file_streaming(path) else { continue };
                    persist_oversized_file(&tx, rel_str, &new_hash, meta.len(), mtime)?;
                    summary.added += 1;
                    continue;
                }
                let Ok(bytes) = std::fs::read(path) else { continue };
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

fn load_existing(conn: &Connection) -> Result<HashMap<String, (i64, String)>, IndexError> {
    let mut stmt = conn.prepare("SELECT path, mtime_unix, content_hash FROM files")?;
    let rows = stmt.query_map([], |row| {
        let path: String = row.get(0)?;
        let mtime: i64 = row.get(1)?;
        let hash: String = row.get(2)?;
        Ok((path, (mtime, hash)))
    })?;

    let mut map = HashMap::new();
    for row in rows {
        let (path, val) = row?;
        map.insert(path, val);
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
