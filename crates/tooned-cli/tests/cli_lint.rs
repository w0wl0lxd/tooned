// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for `tooned lint`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write as _;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn lint_valid_toon_file_exits_0_and_prints_ok() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"a: 1\nb: hello\n").unwrap();

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["lint", file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok: valid TOON"));
}

#[test]
fn lint_valid_uniform_array_exits_0() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"[{a: 1, b: hello}, {a: 2, b: world}]\n").unwrap();

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["lint", file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok: valid TOON"));
}

#[test]
fn lint_invalid_toon_exits_non_zero_with_diagnostic() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"a: \"unterminated string").unwrap();

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["lint", file.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not valid TOON"));
}

#[test]
fn lint_missing_file_exits_non_zero() {
    Command::cargo_bin("tooned")
        .unwrap()
        .args(["lint", "/nonexistent/path/toon"])
        .assert()
        .failure();
}

#[test]
fn lint_uniform_array_tolerates_reordered_keys() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"- a: 1\n  b: hello\n- b: world\n  a: 2\n").unwrap();

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["lint", file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok: valid TOON"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn lint_uniform_array_warns_on_inconsistent_keys() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"- a: 1\n  b: hello\n- a: 2\n").unwrap();

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["lint", file.path().to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("inconsistent key sets"));
}
