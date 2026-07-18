// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Contract test for `tooned token-savings`.

use std::fmt::Write as _;

use assert_cmd::Command;
use predicates::prelude::*;

fn uniform_array_json(rows: usize) -> String {
    let mut s = String::from("[");
    for i in 0..rows {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, r#"{{"id":{i},"name":"row-{i}","active":true,"score":{i}.5}}"#);
    }
    s.push(']');
    s
}

#[test]
fn token_savings_reports_bpe_savings_for_convertible_json() {
    let dir = tempfile::tempdir().unwrap();
    let json = uniform_array_json(20);
    let path = dir.path().join("input.json");
    std::fs::write(&path, &json).unwrap();

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["token-savings", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("bpe-token savings:")
                .and(predicate::str::contains("would convert: yes")),
        );
}

#[test]
fn token_savings_json_reports_savings_fields() {
    let dir = tempfile::tempdir().unwrap();
    let json = uniform_array_json(20);
    let path = dir.path().join("input.json");
    std::fs::write(&path, &json).unwrap();

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["token-savings", "--json", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"would_convert\":true")
                .and(predicate::str::contains("\"bpe-token savings\":").not()),
        );
}

#[test]
fn token_savings_missing_file_exits_non_zero() {
    Command::cargo_bin("tooned")
        .unwrap()
        .args(["token-savings", "/nonexistent/path/to/input.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to read"));
}

#[test]
fn token_savings_missing_file_json_does_not_emit_garbage() {
    Command::cargo_bin("tooned")
        .unwrap()
        .args(["token-savings", "--json", "/nonexistent/path/to/input.json"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty());
}
