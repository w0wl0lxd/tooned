// SPDX-License-Identifier: AGPL-3.0-only

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
    let result = tooned_index::sync(dir.path(), &tooned_index::IndexFilter::default());
    assert!(result.is_err(), "sync with no prior `index` must error, not panic");
}

#[test]
fn sync_skips_rehashing_a_file_whose_mtime_is_unchanged() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("data.json");
    fs::write(&path, uniform_array_json(10)).expect("write fixture");

    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("initial scan_full");
    let before = tooned_index::show_file(dir.path(), "data.json").expect("show_file before sync");

    let summary = tooned_index::sync(dir.path(), &tooned_index::IndexFilter::default()).expect("sync");
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
    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("initial scan_full");
    let before = tooned_index::show_file(dir.path(), "data.json").expect("show_file before edit");

    // Bump mtime forward and change content, simulating a real edit.
    fs::write(&path, uniform_array_json(30)).expect("rewrite fixture with different content");
    let future = SystemTime::now() + Duration::from_mins(2);
    set_mtime(&path, future).expect("set_mtime");

    let summary = tooned_index::sync(dir.path(), &tooned_index::IndexFilter::default()).expect("sync");
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
    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("initial scan_full");

    fs::remove_file(&path).expect("remove fixture file");

    let summary = tooned_index::sync(dir.path(), &tooned_index::IndexFilter::default()).expect("sync");
    assert_eq!(summary.removed, 1);

    let result = tooned_index::show_file(dir.path(), "data.json");
    assert!(result.is_err(), "deleted file's row must be pruned");
}

#[test]
fn sync_adds_a_newly_created_file() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("first.json"), uniform_array_json(5)).expect("write fixture");
    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("initial scan_full");

    fs::write(dir.path().join("second.json"), uniform_array_json(5)).expect("write new fixture");
    let summary = tooned_index::sync(dir.path(), &tooned_index::IndexFilter::default()).expect("sync");
    assert_eq!(summary.added, 1);

    assert!(tooned_index::show_file(dir.path(), "second.json").is_ok());
}

#[test]
fn sync_with_exclude_does_not_prune_excluded_files_that_still_exist() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("data.json"), uniform_array_json(5)).expect("write fixture");
    fs::write(dir.path().join("excluded.json"), uniform_array_json(5)).expect("write fixture");

    // Initial scan without filter
    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("initial scan_full");
    assert!(tooned_index::show_file(dir.path(), "excluded.json").is_ok(), "excluded.json should be indexed initially");

    // Sync with exclude filter - excluded.json should still exist on disk
    let filter = tooned_index::IndexFilter {
        type_filter: None,
        excludes: vec!["excluded.json".to_string()],
    };
    let summary = tooned_index::sync(dir.path(), &filter).expect("sync");
    
    // excluded.json should NOT be pruned because it still exists on disk
    assert!(tooned_index::show_file(dir.path(), "excluded.json").is_ok(), "excluded.json must NOT be pruned when it still exists");
    assert_eq!(summary.removed, 0, "no files should be removed");
}

#[test]
fn sync_with_type_filter_respects_filter() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("data.json"), uniform_array_json(5)).expect("write fixture");
    fs::write(dir.path().join("config.toml"), "[section]\nkey = \"value\"").expect("write fixture");

    // Initial scan without filter
    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("initial scan_full");
    assert!(tooned_index::show_file(dir.path(), "config.toml").is_ok(), "config.toml should be indexed initially");

    // Sync with type filter - only JSON files should be considered
    let filter = tooned_index::IndexFilter {
        type_filter: Some(tooned_index::DocTypeFilter::Json),
        excludes: vec![],
    };
    let summary = tooned_index::sync(dir.path(), &filter).expect("sync");
    
    // config.toml should NOT be pruned because it still exists on disk and matches no filter
    assert!(tooned_index::show_file(dir.path(), "config.toml").is_ok(), "config.toml must NOT be pruned when it still exists");
    assert_eq!(summary.removed, 0, "no files should be removed");
}
