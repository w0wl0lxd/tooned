//! Contract test for `tooned check` (T041).
//! See `specs/001-adaptive-toon-conversion/contracts/cli.md`.

use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;

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

#[allow(clippy::expect_used)]
fn write_fixture(dir: &tempfile::TempDir, name: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let mut f = fs::File::create(&path).expect("create fixture file");
    f.write_all(contents.as_bytes()).expect("write fixture file");
    path
}

#[test]
fn check_prints_doc_type_shape_and_savings_with_no_side_effect() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json = uniform_array_json(20);
    let path = write_fixture(&dir, "input.json", &json);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["check", path.to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Json")
                .and(predicate::str::contains("UniformArrayOfObjects"))
                .and(predicate::str::contains("convertible: yes")),
        );

    // Dry-run: no converted-output file/side effect written anywhere near
    // the source.
    let entries: Vec<_> = fs::read_dir(dir.path()).expect("read dir").collect();
    assert_eq!(entries.len(), 1, "check must not create any additional files");
}

#[test]
fn check_precise_flag_reports_a_bpe_token_savings_percentage() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json = uniform_array_json(20);
    let path = write_fixture(&dir, "input.json", &json);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["check", path.to_str().expect("utf8 path"), "--precise"])
        .assert()
        .success()
        .stdout(
            predicate::str::is_match(r"precise \(BPE-token\) savings: \d+\.\d%")
                .expect("valid regex"),
        );
}

#[test]
fn check_without_precise_flag_omits_the_bpe_token_savings_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json = uniform_array_json(20);
    let path = write_fixture(&dir, "input.json", &json);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["check", path.to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("precise (BPE-token) savings").not());
}

#[test]
fn check_not_convertible_input_still_exits_0() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_fixture(&dir, "input.txt", "just some prose, nothing structured here");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["check", path.to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("convertible: no"));
}

#[test]
fn check_missing_file_still_exits_0_per_contract() {
    // Regression test: contracts/cli.md documents `check`'s exit code as
    // "0 always (a 'not convertible' result is not a CLI error)", with no
    // I/O-error exception (unlike `convert`, which explicitly documents
    // exit 2 for input not found/unreadable). A missing/unreadable input
    // path must not hard-exit non-zero here.
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("does-not-exist.json");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["check", missing.to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("convertible: no"));
}

#[test]
fn check_format_hint_overrides_ambiguous_content_sniffing() {
    // A single-line, single-row delimited file has no second line for
    // `sniff_delimited` to confirm a consistent comma count against, so
    // content-sniffing alone reports it as an unrecognized doc type -- this
    // is exactly the CLI-side gap the finding calls out (MCP's
    // `format_hint` could already force this; the CLI had no equivalent
    // flag at all).
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_fixture(&dir, "ambiguous.txt", "1,2,3");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["check", path.to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("doc type: unknown"));

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["check", path.to_str().expect("utf8 path"), "--format-hint", "csv"])
        .assert()
        .success()
        .stdout(predicate::str::contains("doc type: Csv"));
}
