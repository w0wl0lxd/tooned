// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Contract test for `tooned pipe` (T042).
//! See `specs/001-adaptive-toon-conversion/contracts/cli.md`.

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
fn pipe_adaptively_converts_stdin_to_stdout() {
    let json = uniform_array_json(20);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .arg("pipe")
        .write_stdin(json)
        .assert()
        .success()
        .stdout(predicate::str::contains("id,name,active,score"));
}

#[test]
fn pipe_passes_through_non_json_stdin_unchanged() {
    let prose = "just some prose, nothing structured here";

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .arg("pipe")
        .write_stdin(prose)
        .assert()
        .success()
        .stdout(predicate::eq(prose));
}

#[test]
fn pipe_writes_converted_output_to_file() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.toon");
    let json = uniform_array_json(20);

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["pipe", "--out", out.to_str().unwrap()])
        .write_stdin(json)
        .assert()
        .success();

    let contents = std::fs::read_to_string(&out).unwrap();
    assert!(
        contents.contains("id,name,active,score"),
        "file should contain converted TOON: {contents}"
    );
}

#[test]
fn pipe_streams_oversized_input_to_file() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.txt");
    let prose = "hello world"; // > --max-bytes 8, so it streams verbatim.

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["pipe", "--max-bytes", "8", "--out", out.to_str().unwrap()])
        .write_stdin(prose)
        .assert()
        .success();

    assert_eq!(std::fs::read_to_string(&out).unwrap(), prose);
}

#[test]
fn pipe_always_exits_0_even_on_malformed_json() {
    let malformed = "{\"a\": not valid";

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .arg("pipe")
        .write_stdin(malformed)
        .assert()
        .success()
        .stdout(predicate::eq(malformed));
}
