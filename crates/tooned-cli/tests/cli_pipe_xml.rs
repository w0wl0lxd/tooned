//! Contract tests for `tooned pipe` with XML input.
//! See `specs/002-xml-conversion/contracts/cli.md`.

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn pipe_adaptively_converts_xml_stdin_to_stdout() {
    let xml = common::xml::xml_record_list(10);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .arg("pipe")
        .write_stdin(xml)
        .assert()
        .success()
        .stdout(predicate::str::contains("record[10]{@id,@name,@active,@score}:"));
}

#[test]
fn pipe_passes_through_xml_that_does_not_convert_well() {
    let xml = common::xml::long_text_xml(1000);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .arg("pipe")
        .write_stdin(xml.clone())
        .assert()
        .success()
        .stdout(predicate::eq(xml));
}
