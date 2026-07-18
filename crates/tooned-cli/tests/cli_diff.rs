// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned diff` integration tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write as _;

use assert_cmd::Command;
use predicates::prelude::*;

fn write_fixture(dir: &tempfile::TempDir, name: &str, content: &[u8]) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let mut file = std::fs::File::create(&path).expect("create fixture");
    file.write_all(content).expect("write fixture");
    path
}

#[test]
fn diff_reports_no_diff_for_convertible_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json = br#"{"users":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}]}"#;
    let path = write_fixture(&dir, "input.json", json);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["diff", path.to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("no diff"));
}

#[test]
fn diff_json_reports_equal_true_for_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json = br#"{"users":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}]}"#;
    let path = write_fixture(&dir, "input.json", json);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["diff", "--json", path.to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"equal\":true"));
}

#[test]
fn diff_reports_non_convertible_input_with_exit_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Plain prose will be a passthrough (not converted).
    let path = write_fixture(&dir, "plain.txt", b"this is just plain text");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["diff", path.to_str().expect("utf8 path")])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("not converted"));
}
