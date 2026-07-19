// SPDX-License-Identifier: AGPL-3.0-only

//! Integration test: first index creation appends `.tooned/` to the
//! project's `.gitignore` (creating it if absent); running index again does
//! not duplicate the entry (T052).

use std::fs;

use tempfile::tempdir;

#[test]
fn first_scan_creates_gitignore_with_tooned_entry_when_absent() {
    let dir = tempdir().expect("tempdir");
    assert!(!dir.path().join(".gitignore").exists());

    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("scan_full");

    let contents = fs::read_to_string(dir.path().join(".gitignore")).expect("read .gitignore");
    assert!(contents.lines().any(|l| l.trim() == ".tooned/"));
}

#[test]
fn first_scan_appends_tooned_entry_to_an_existing_gitignore() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join(".gitignore"), "target/\nnode_modules/\n").expect("seed .gitignore");

    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("scan_full");

    let contents = fs::read_to_string(dir.path().join(".gitignore")).expect("read .gitignore");
    assert!(contents.lines().any(|l| l.trim() == "target/"), "existing entries must be preserved");
    assert!(contents.lines().any(|l| l.trim() == "node_modules/"));
    assert!(contents.lines().any(|l| l.trim() == ".tooned/"));
}

#[test]
fn running_index_twice_does_not_duplicate_the_gitignore_entry() {
    let dir = tempdir().expect("tempdir");

    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default())
        .expect("first scan_full");
    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default())
        .expect("second scan_full");

    let contents = fs::read_to_string(dir.path().join(".gitignore")).expect("read .gitignore");
    let count = contents.lines().filter(|l| l.trim() == ".tooned/").count();
    assert_eq!(count, 1, ".tooned/ must appear exactly once, got:\n{contents}");
}

#[test]
fn an_existing_gitignore_entry_is_not_duplicated() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join(".gitignore"), ".tooned/\n")
        .expect("seed .gitignore already covering it");

    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("scan_full");

    let contents = fs::read_to_string(dir.path().join(".gitignore")).expect("read .gitignore");
    let count = contents.lines().filter(|l| l.trim() == ".tooned/").count();
    assert_eq!(count, 1);
}

#[test]
fn disabling_gitignore_respect_indexes_ignored_files() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("kept.json"), "{\"a\":1}").expect("write kept");
    fs::write(dir.path().join("ignored.json"), "{\"b\":2}").expect("write ignored");
    fs::write(dir.path().join(".gitignore"), "ignored.json\n").expect("write .gitignore");

    let default_filter = tooned_index::IndexFilter::default();
    let no_gitignore_filter =
        tooned_index::IndexFilter { respect_gitignore: false, ..default_filter.clone() };

    let default_summary =
        tooned_index::scan_full(dir.path(), &default_filter).expect("scan with gitignore");
    assert_eq!(default_summary.files_scanned, 1, "default scan should skip ignored.json");
    assert_eq!(default_summary.files_classified, 1);
    assert!(tooned_index::show_file(dir.path(), "kept.json").is_ok());
    assert!(
        tooned_index::show_file(dir.path(), "ignored.json").is_err(),
        "ignored.json should not be indexed by default"
    );

    let no_gitignore_summary =
        tooned_index::scan_full(dir.path(), &no_gitignore_filter).expect("scan without gitignore");
    assert_eq!(
        no_gitignore_summary.files_scanned, 2,
        "--no-gitignore scan should include ignored.json"
    );
    assert_eq!(no_gitignore_summary.files_classified, 2);
    assert!(tooned_index::show_file(dir.path(), "ignored.json").is_ok());
}
