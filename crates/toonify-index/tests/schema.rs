//! Unit tests for SQLite schema creation (T049).
//! `meta`/`files`/`shapes`/`conversions` per data-model.md's "Project Index" section.

use rusqlite::Connection;
use tempfile::tempdir;

fn table_names(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?;
    stmt.query_map([], |row| row.get::<_, String>(0))?.collect()
}

#[test]
fn open_index_creates_all_four_tables() {
    let dir = tempdir().expect("tempdir");
    let conn = tooned_index::open_index(dir.path()).expect("open index");
    let tables = table_names(&conn).expect("table_names");
    for expected in ["meta", "files", "shapes", "conversions"] {
        assert!(tables.iter().any(|t| t == expected), "missing table {expected}, have {tables:?}");
    }
}

#[test]
fn open_index_bootstraps_schema_version_in_meta() {
    let dir = tempdir().expect("tempdir");
    let conn = tooned_index::open_index(dir.path()).expect("open index");
    let version: String = conn
        .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |row| row.get(0))
        .expect("schema_version row present");
    assert!(!version.is_empty());
}

#[test]
fn open_index_bootstraps_created_at_in_meta() {
    let dir = tempdir().expect("tempdir");
    let conn = tooned_index::open_index(dir.path()).expect("open index");
    let created_at: String = conn
        .query_row("SELECT value FROM meta WHERE key = 'created_at'", [], |row| row.get(0))
        .expect("created_at row present");
    assert!(!created_at.is_empty());
}

#[test]
fn reopening_the_index_does_not_duplicate_or_reset_meta_bootstrap() {
    let dir = tempdir().expect("tempdir");
    {
        let conn = tooned_index::open_index(dir.path()).expect("open index (first time)");
        conn.execute("INSERT INTO meta (key, value) VALUES ('marker', 'present')", [])
            .expect("insert marker row");
    }
    // Re-opening (as `scan`/`sync`/`status` all do) must not wipe existing
    // `meta` rows, and `CREATE TABLE IF NOT EXISTS` + `INSERT OR IGNORE`
    // bootstrap must be safely idempotent.
    let conn = tooned_index::open_index(dir.path()).expect("open index (second time)");
    let marker: String = conn
        .query_row("SELECT value FROM meta WHERE key = 'marker'", [], |row| row.get(0))
        .expect("marker row survives reopen");
    assert_eq!(marker, "present");

    let version_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM meta WHERE key = 'schema_version'", [], |row| row.get(0))
        .expect("count schema_version rows");
    assert_eq!(version_count, 1, "schema_version must not be duplicated on reopen");
}

#[test]
fn foreign_keys_cascade_from_files_to_shapes_and_conversions() {
    let dir = tempdir().expect("tempdir");
    let conn = tooned_index::open_index(dir.path()).expect("open index");

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

    let shape_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM shapes", [], |row| row.get(0)).expect("count shapes");
    let conversion_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM conversions", [], |row| row.get(0))
        .expect("count conversions");
    assert_eq!(shape_count, 0, "shapes row must cascade-delete with its file");
    assert_eq!(conversion_count, 0, "conversions row must cascade-delete with its file");
}
