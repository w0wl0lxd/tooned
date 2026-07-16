//! Integration test: incremental sync skips re-hashing an unchanged file,
//! re-classifies a changed one, and prunes rows for deleted files (T051).

use std::fmt::Write as _;
use std::fs;
use std::time::{Duration, SystemTime};

use tempfile::tempdir;

fn uniform_array_json(rows: usize) -> String {
    let mut s = String::from("[");
    for i in 0..rows {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, r#"{{"id":{i},"name":"row-{i}","active":true,"score":{i}}}"#);
    }
    s.push(']');
    s
}

fn set_mtime(path: &std::path::Path, when: SystemTime) -> std::io::Result<()> {
    // Open with write permission; Windows requires a writable handle to call
    // `SetFileTime` via `set_modified`.
    let file = fs::OpenOptions::new().write(true).open(path)?;
    file.set_modified(when)
}

#[test]
fn sync_without_an_existing_index_reports_no_index_error() {
    let dir = tempdir().expect("tempdir");
    let result = tooned_index::sync(dir.path());
    assert!(result.is_err(), "sync with no prior `index` must error, not panic");
}

#[test]
fn sync_skips_rehashing_a_file_whose_mtime_is_unchanged() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("data.json");
    fs::write(&path, uniform_array_json(10)).expect("write fixture");

    tooned_index::scan_full(dir.path()).expect("initial scan_full");
    let before = tooned_index::show_file(dir.path(), "data.json").expect("show_file before sync");

    let summary = tooned_index::sync(dir.path()).expect("sync");
    assert_eq!(summary.unchanged, 1);
    assert_eq!(summary.updated, 0);
    assert_eq!(summary.added, 0);

    let after = tooned_index::show_file(dir.path(), "data.json").expect("show_file after sync");
    assert_eq!(before.file.content_hash, after.file.content_hash);
    assert_eq!(
        before.file.scanned_at, after.file.scanned_at,
        "unchanged file must not be re-scanned"
    );
}

#[test]
fn sync_reclassifies_a_file_whose_content_changed() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("data.json");
    fs::write(&path, uniform_array_json(10)).expect("write fixture");
    tooned_index::scan_full(dir.path()).expect("initial scan_full");
    let before = tooned_index::show_file(dir.path(), "data.json").expect("show_file before edit");

    // Bump mtime forward and change content, simulating a real edit.
    fs::write(&path, uniform_array_json(30)).expect("rewrite fixture with different content");
    let future = SystemTime::now() + Duration::from_mins(2);
    set_mtime(&path, future).expect("set_mtime");

    let summary = tooned_index::sync(dir.path()).expect("sync");
    assert_eq!(summary.updated, 1);
    assert_eq!(summary.unchanged, 0);

    let after = tooned_index::show_file(dir.path(), "data.json").expect("show_file after edit");
    assert_ne!(before.file.content_hash, after.file.content_hash);
}

#[test]
fn sync_prunes_rows_for_deleted_files() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("data.json");
    fs::write(&path, uniform_array_json(10)).expect("write fixture");
    tooned_index::scan_full(dir.path()).expect("initial scan_full");

    fs::remove_file(&path).expect("remove fixture file");

    let summary = tooned_index::sync(dir.path()).expect("sync");
    assert_eq!(summary.removed, 1);

    let result = tooned_index::show_file(dir.path(), "data.json");
    assert!(result.is_err(), "deleted file's row must be pruned");
}

#[test]
fn sync_adds_a_newly_created_file() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("first.json"), uniform_array_json(5)).expect("write fixture");
    tooned_index::scan_full(dir.path()).expect("initial scan_full");

    fs::write(dir.path().join("second.json"), uniform_array_json(5)).expect("write new fixture");
    let summary = tooned_index::sync(dir.path()).expect("sync");
    assert_eq!(summary.added, 1);

    assert!(tooned_index::show_file(dir.path(), "second.json").is_ok());
}
