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

    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("first scan_full");
    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("second scan_full");

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
