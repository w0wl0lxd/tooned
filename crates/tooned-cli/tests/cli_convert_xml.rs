// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Contract tests for `tooned convert` with XML input.
//! See `specs/002-xml-conversion/contracts/cli.md`.

mod common;

use std::fs;
use std::io::Write as _;

use assert_cmd::Command;
use predicates::prelude::*;

#[allow(clippy::expect_used)]
fn write_fixture(dir: &tempfile::TempDir, name: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let mut f = fs::File::create(&path).expect("create fixture file");
    f.write_all(contents.as_bytes()).expect("write fixture file");
    path
}

#[test]
fn convert_to_toon_converts_xml_record_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let xml = common::xml::xml_record_list(20);
    let path = write_fixture(&dir, "input.xml", &xml);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "toon"])
        .assert()
        .success()
        .stdout(predicate::str::contains("record[20]{@id,@name,@active,@score}:"));
}

#[test]
fn convert_to_json_decodes_a_toon_file_back_to_compact_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let xml = common::xml::xml_record_list(5);
    let xml_path = write_fixture(&dir, "input.xml", &xml);
    let toon_path = dir.path().join("input.toon");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args([
            "convert",
            xml_path.to_str().expect("utf8 path"),
            "--to",
            "toon",
            "--out",
            toon_path.to_str().expect("utf8 path"),
        ])
        .assert()
        .success();

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", toon_path.to_str().expect("utf8 path"), "--to", "json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(r#"{"data":{"record":["#)
                .and(predicate::str::contains(r#""@id":"0""#))
                .and(predicate::str::contains(r#""@name":"row-0""#))
                .and(predicate::str::contains(r#""@active":"true""#))
                .and(predicate::str::contains(r#""@score":"0""#)),
        );
}
