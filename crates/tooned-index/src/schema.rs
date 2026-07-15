//! SQLite schema for `.tooned/index.db`: table creation +
//! `meta.schema_version` bootstrap (T055). Exact schema per data-model.md's
//! "Project Index" section.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::IndexError;

/// Current schema version, stamped into `meta` on first creation.
pub const SCHEMA_VERSION: &str = "1";

/// One row of the `files` table.
#[derive(Debug, Clone, PartialEq)]
pub struct FileRecord {
    pub path: String,
    pub size_bytes: i64,
    pub mtime_unix: i64,
    pub content_hash: String,
    pub doc_type: Option<String>,
    pub scanned_at: i64,
}

/// One row of the `shapes` table.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeRecord {
    pub path: String,
    pub json_pointer: String,
    pub shape_class: String,
    pub uniformity_pct: Option<f64>,
    pub sampled_count: Option<i64>,
}

/// One row of the `conversions` table.
#[derive(Debug, Clone, PartialEq)]
pub struct ConversionRecord {
    pub path: String,
    pub json_pointer: String,
    pub json_bytes: i64,
    pub toon_bytes: i64,
    pub savings_pct: f64,
    pub cached_toon_text: Option<String>,
    pub computed_at: i64,
}

/// Path to the SQLite index file for `project_root` (`.tooned/index.db`,
/// per data-model.md).
pub fn index_db_path(project_root: &Path) -> PathBuf {
    project_root.join(".tooned").join("index.db")
}

/// Whether an index database file already exists for `project_root`. A
/// cheap existence probe used by callers (`status`, `sync`, `show_file`,
/// `stats`) to distinguish "no index yet" from a real I/O error without
/// having to open the database first.
pub fn index_exists(project_root: &Path) -> bool {
    index_db_path(project_root).is_file()
}

/// Opens a connection to `project_root`'s `.tooned/index.db`, creating the
/// `.tooned/` directory and the database file if they don't exist yet, and
/// ensures the schema (all four tables + the `meta` bootstrap rows) is
/// present. Safe to call repeatedly -- table creation and the `meta`
/// bootstrap are both idempotent.
///
/// Crate-private: `Connection`/`open_index` are deliberately never
/// re-exported from the crate root (`lib.rs`). The intended public surface
/// of this crate is the high-level `scan_full`/`sync`/`status`/`show_file`/
/// `stats` API; leaking a raw `rusqlite::Connection` (a third-party
/// dependency type, pinned to its exact version) alongside that would tempt
/// future callers to run ad hoc SQL against the internal schema instead of
/// extending the intended API -- the "parallel implementation" pattern
/// constitution Principle V forbids for `tooned-core`, and the same
/// concern applies here. It would also make any future change to how the
/// DB is opened/pooled (or a swap away from `rusqlite` entirely) a breaking
/// public-API change for this crate rather than an internal detail.
pub(crate) fn open_index(project_root: &Path) -> Result<Connection, IndexError> {
    let db_path = index_db_path(project_root);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)?;
    // Wait up to 5 seconds when the database is locked by a concurrent
    // process (e.g. another `tooned index` or `sync` run) instead of failing
    // immediately with `SQLITE_BUSY`.
    conn.busy_timeout(Duration::from_secs(5))?;
    // Required for `ON DELETE CASCADE` (shapes/conversions -> files) to
    // actually take effect -- SQLite has foreign key enforcement off by
    // default per-connection.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    create_schema(&conn)?;
    Ok(conn)
}

/// Creates every table (`meta`/`files`/`shapes`/`conversions`) if it
/// doesn't already exist, and bootstraps `meta.schema_version` /
/// `meta.created_at`. Idempotent: safe to call on every `open_index`.
pub(crate) fn create_schema(conn: &Connection) -> Result<(), IndexError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS files (
            path         TEXT PRIMARY KEY,
            size_bytes   INTEGER NOT NULL,
            mtime_unix   INTEGER NOT NULL,
            content_hash TEXT NOT NULL,
            doc_type     TEXT,
            scanned_at   INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS shapes (
            path           TEXT NOT NULL,
            json_pointer   TEXT NOT NULL,
            shape_class    TEXT NOT NULL,
            uniformity_pct REAL,
            sampled_count  INTEGER,
            PRIMARY KEY (path, json_pointer),
            FOREIGN KEY (path) REFERENCES files(path) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS conversions (
            path             TEXT NOT NULL,
            json_pointer     TEXT NOT NULL,
            json_bytes       INTEGER NOT NULL,
            toon_bytes       INTEGER NOT NULL,
            savings_pct      REAL NOT NULL,
            cached_toon_text TEXT,
            computed_at      INTEGER NOT NULL,
            PRIMARY KEY (path, json_pointer),
            FOREIGN KEY (path) REFERENCES files(path) ON DELETE CASCADE
        );
        ",
    )?;
    bootstrap_meta(conn)
}

fn bootstrap_meta(conn: &Connection) -> Result<(), IndexError> {
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)",
        [SCHEMA_VERSION],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES ('created_at', ?1)",
        [now_unix().to_string()],
    )?;
    Ok(())
}

/// Current Unix timestamp (seconds), clamped to `0` if the system clock is
/// somehow set before the epoch -- never a panic on a clock read failure.
pub(crate) fn now_unix() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => saturating_i64(d.as_secs()),
        Err(_) => 0,
    }
}

/// A file's modification time as a Unix timestamp (seconds), `0` if
/// unavailable or unrepresentable -- never a panic (constitution Principle
/// I: this is reachable from arbitrary filesystem entries encountered
/// during a scan).
pub(crate) fn file_mtime_unix(meta: &std::fs::Metadata) -> i64 {
    match meta.modified() {
        Ok(t) => match t.duration_since(UNIX_EPOCH) {
            Ok(d) => saturating_i64(d.as_secs()),
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

/// Saturating `u64` -> `i64` conversion for SQLite storage (SQLite integers
/// are signed 64-bit). Every value this project actually converts (byte
/// sizes, sample counts, Unix timestamps) is astronomically far below
/// `i64::MAX`, so saturation is an inert, always-safe fallback -- this is
/// explicit, bounds-checked conversion (never a silent wraparound), not a
/// bare `as` cast; the scoped `cast_possible_wrap` allow below is only
/// needed because clippy can't see the preceding bounds check.
#[allow(clippy::cast_possible_wrap)]
pub(crate) fn saturating_i64(n: u64) -> i64 {
    if n > i64::MAX as u64 { i64::MAX } else { n as i64 }
}

#[cfg(test)]
mod tests {
    //! Unit tests for SQLite schema creation (T049).
    //! `meta`/`files`/`shapes`/`conversions` per data-model.md's "Project
    //! Index" section. Kept as an in-crate unit test module (rather than an
    //! external `tests/schema.rs` integration test) specifically because
    //! `open_index`/`Connection` are crate-private (finding: this crate must
    //! not leak `rusqlite::Connection` as part of its public API) -- only
    //! code inside the crate can exercise them directly.
    use tempfile::tempdir;

    use super::*;

    fn table_names(conn: &Connection) -> rusqlite::Result<Vec<String>> {
        let mut stmt =
            conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?.collect()
    }

    #[test]
    fn open_index_creates_all_four_tables() {
        let dir = tempdir().expect("tempdir");
        let conn = open_index(dir.path()).expect("open index");
        let tables = table_names(&conn).expect("table_names");
        for expected in ["meta", "files", "shapes", "conversions"] {
            assert!(
                tables.iter().any(|t| t == expected),
                "missing table {expected}, have {tables:?}"
            );
        }
    }

    #[test]
    fn open_index_bootstraps_schema_version_in_meta() {
        let dir = tempdir().expect("tempdir");
        let conn = open_index(dir.path()).expect("open index");
        let version: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |row| row.get(0))
            .expect("schema_version row present");
        assert!(!version.is_empty());
    }

    #[test]
    fn open_index_bootstraps_created_at_in_meta() {
        let dir = tempdir().expect("tempdir");
        let conn = open_index(dir.path()).expect("open index");
        let created_at: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'created_at'", [], |row| row.get(0))
            .expect("created_at row present");
        assert!(!created_at.is_empty());
    }

    #[test]
    fn reopening_the_index_does_not_duplicate_or_reset_meta_bootstrap() {
        let dir = tempdir().expect("tempdir");
        {
            let conn = open_index(dir.path()).expect("open index (first time)");
            conn.execute("INSERT INTO meta (key, value) VALUES ('marker', 'present')", [])
                .expect("insert marker row");
        }
        // Re-opening (as `scan`/`sync`/`status` all do) must not wipe
        // existing `meta` rows, and `CREATE TABLE IF NOT EXISTS` +
        // `INSERT OR IGNORE` bootstrap must be safely idempotent.
        let conn = open_index(dir.path()).expect("open index (second time)");
        let marker: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'marker'", [], |row| row.get(0))
            .expect("marker row survives reopen");
        assert_eq!(marker, "present");

        let version_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM meta WHERE key = 'schema_version'", [], |row| {
                row.get(0)
            })
            .expect("count schema_version rows");
        assert_eq!(version_count, 1, "schema_version must not be duplicated on reopen");
    }

    #[test]
    fn foreign_keys_cascade_from_files_to_shapes_and_conversions() {
        let dir = tempdir().expect("tempdir");
        let conn = open_index(dir.path()).expect("open index");

        conn.execute(
            "INSERT INTO files (path, size_bytes, mtime_unix, content_hash, doc_type, scanned_at)
             VALUES ('a.json', 10, 100, 'deadbeef', 'json', 100)",
            [],
        )
        .expect("insert file row");
        conn.execute(
            "INSERT INTO shapes (path, json_pointer, shape_class, uniformity_pct, sampled_count)
             VALUES ('a.json', '', 'uniform', 1.0, 3)",
            [],
        )
        .expect("insert shape row");
        conn.execute(
            "INSERT INTO conversions (path, json_pointer, json_bytes, toon_bytes, savings_pct, cached_toon_text, computed_at)
             VALUES ('a.json', '', 100, 50, 50.0, NULL, 100)",
            [],
        )
        .expect("insert conversion row");

        conn.execute("DELETE FROM files WHERE path = 'a.json'", []).expect("delete file row");

        let shape_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM shapes", [], |row| row.get(0))
            .expect("count shapes");
        let conversion_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM conversions", [], |row| row.get(0))
            .expect("count conversions");
        assert_eq!(shape_count, 0, "shapes row must cascade-delete with its file");
        assert_eq!(conversion_count, 0, "conversions row must cascade-delete with its file");
    }
}
