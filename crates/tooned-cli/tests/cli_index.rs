// SPDX-License-Identifier: AGPL-3.0-only

//! Contract test for `tooned index` / `index sync` / `index status` /
//! `index show <file>` (T054).
//! See `specs/001-adaptive-toon-conversion/contracts/cli.md`.

use std::fmt::Write as _;
use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

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

#[test]
fn index_status_against_a_non_indexed_project_reports_gracefully_and_exits_0() {
    let dir = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["index", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no index").or(predicate::str::contains("No index")));
}

#[test]
fn index_show_against_a_non_indexed_project_reports_gracefully_not_panic() {
    let dir = tempfile::tempdir().expect("tempdir");

    let assert = Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["index", "show", "nonexistent.json"])
        .assert();
    // Never a panic/crash (no abort signal) -- a clean non-zero exit
    // reporting "not indexed" is the graceful outcome the contract requires.
    assert
        .failure()
        .code(predicate::eq(2))
        .stderr(predicate::str::contains("not indexed").or(predicate::str::contains("no index")));
}

#[test]
fn index_sync_against_a_non_indexed_project_reports_gracefully_not_panic() {
    let dir = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["index", "sync"])
        .assert()
        .failure()
        .code(predicate::eq(1))
        .stderr(predicate::str::contains("index"));
}

#[test]
fn index_full_scan_then_status_then_show_report_correct_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("data.json"), uniform_array_json(10)).expect("write fixture");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .arg("index")
        .assert()
        .success()
        .stdout(predicate::str::contains("1"));

    assert!(dir.path().join(".tooned").join("index.db").exists());

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["index", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1"));

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["index", "show", "data.json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("data.json").and(predicate::str::contains("json")));
}

#[test]
fn index_full_scan_appends_tooned_to_gitignore() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("data.json"), uniform_array_json(10)).expect("write fixture");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .arg("index")
        .assert()
        .success();

    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).expect("read .gitignore");
    assert!(gitignore.lines().any(|l| l.trim() == ".tooned/"));
}

#[test]
fn index_path_not_found_fails_with_exit_code_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("does-not-exist");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["index", missing.to_str().expect("utf8 path")])
        .assert()
        .failure()
        .code(predicate::eq(2));
}
