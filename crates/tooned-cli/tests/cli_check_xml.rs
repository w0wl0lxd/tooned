// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Contract tests for `tooned check` with XML input.
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
fn check_prints_doc_type_shape_and_savings_for_xml() {
    let dir = tempfile::tempdir().expect("tempdir");
    let xml = common::xml::xml_record_list(20);
    let path = write_fixture(&dir, "input.xml", &xml);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["check", path.to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("doc type: Xml")
                .and(predicate::str::contains("shape: Scalar"))
                .and(predicate::str::contains("input bytes:"))
                .and(predicate::str::contains("json bytes:"))
                .and(predicate::str::contains("toon bytes:"))
                .and(predicate::str::contains("savings:"))
                .and(predicate::str::contains("convertible: yes")),
        );
}

#[test]
fn check_format_hint_xml_overrides_content_on_non_xml_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_fixture(&dir, "ambiguous.txt", "just some prose, nothing structured here");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["check", path.to_str().expect("utf8 path"), "--format-hint", "xml"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("doc type: Xml")
                .and(predicate::str::contains("convertible: no"))
                .and(predicate::str::contains("reason: ParseFailed")),
        );
}
