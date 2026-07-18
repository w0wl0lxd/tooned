// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned metrics` and `tooned heatmap` integration tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write as _;
use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

fn write_fixture(dir: &tempfile::TempDir, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    let mut file = std::fs::File::create(&path).expect("create fixture");
    file.write_all(content).expect("write fixture");
    path
}

fn setup_project_with_index(dir: &tempfile::TempDir) -> PathBuf {
    let json = br#"{"users":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}]}"#;
    let _ = write_fixture(dir, "data.json", json);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["index", "."])
        .assert()
        .success();

    dir.path().join("data.json")
}

#[test]
fn metrics_summary_reports_empty_ledger_gracefully() {
    let dir = tempfile::tempdir().expect("tempdir");
    let _ = setup_project_with_index(&dir);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["metrics", "summary"])
        .assert()
        .success()
        .stdout(predicate::str::contains("saved").or(predicate::str::contains("Total")));
}

#[test]
fn metrics_summary_json_contains_summary_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = setup_project_with_index(&dir);

    // Record a conversion event.
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "toon"])
        .assert()
        .success();

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["metrics", "summary"])
        .assert()
        .success()
        .stdout(predicate::str::contains("conversions"));
}

#[test]
fn heatmap_renders_calendar_for_project_ledger() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = setup_project_with_index(&dir);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "toon"])
        .assert()
        .success();

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["heatmap"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sun").or(predicate::str::contains("Mon")));
}
