//! SQLite schema for `.tooned/index.db`: table creation +
//! `meta.schema_version` bootstrap (T055). Exact schema per data-model.md's
//! "Project Index" section.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
pub fn open_index(project_root: &Path) -> Result<Connection, IndexError> {
    let db_path = index_db_path(project_root);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)?;
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
pub fn create_schema(conn: &Connection) -> Result<(), IndexError> {
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
