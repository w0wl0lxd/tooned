//! Contract tests for `tooned convert` (T038-T040).
//! See `specs/001-adaptive-toon-conversion/contracts/cli.md`.

use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;

/// A uniform array-of-objects payload big enough to reliably beat the
/// default 2% margin when re-encoded as TOON.
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
fn convert_to_toon_writes_converted_content_to_stdout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json = uniform_array_json(20);
    let path = write_fixture(&dir, "input.json", &json);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "toon"])
        .assert()
        .success()
        // TOON encodes uniform array-of-objects with a tabular header;
        // this is a robust signal we got real TOON output, not the raw
        // JSON echoed back.
        .stdout(predicate::str::contains("id,name,active,score"));
}

#[test]
fn convert_to_json_decodes_a_toon_file_back_to_compact_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let toon = "a: 1\nb: hello\n";
    let path = write_fixture(&dir, "input.toon", toon);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(r#""a":1"#).and(predicate::str::contains(r#""b":"hello""#)),
        );
}

#[test]
fn convert_never_mutates_the_source_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json = uniform_array_json(20);
    let path = write_fixture(&dir, "input.json", &json);

    let before_bytes = fs::read(&path).expect("read fixture before");
    let before_mtime = fs::metadata(&path).expect("stat before").modified().expect("mtime before");

    // A small sleep so that, if the source were touched, mtime would
    // observably move forward (filesystem mtime resolution varies).
    std::thread::sleep(Duration::from_millis(20));

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "toon"])
        .assert()
        .success();

    let after_bytes = fs::read(&path).expect("read fixture after");
    let after_mtime = fs::metadata(&path).expect("stat after").modified().expect("mtime after");

    assert_eq!(before_bytes, after_bytes, "convert must never mutate the source file's bytes");
    assert_eq!(before_mtime, after_mtime, "convert must never mutate the source file's mtime");
}

#[test]
fn convert_missing_file_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("does-not-exist.json");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", missing.to_str().expect("utf8 path")])
        .assert()
        .code(2);
}

#[test]
fn convert_does_not_truncate_source_when_out_is_a_hardlink() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json = uniform_array_json(20);
    let input = write_fixture(&dir, "input.json", &json);
    let link = dir.path().join("link.json");

    // Hard links are supported on most, but not all, filesystems. If the
    // filesystem cannot create one, skip this test rather than fail.
    if let Err(err) = fs::hard_link(&input, &link) {
        eprintln!("skipping hardlink test: filesystem does not support hard links: {err}");
        return;
    }

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args([
            "convert",
            input.to_str().expect("utf8 path"),
            "--out",
            link.to_str().expect("utf8 path"),
            "--to",
            "toon",
        ])
        .assert()
        .success();

    let input_bytes = fs::read(&input).expect("read input after");
    let link_bytes = fs::read(&link).expect("read link after");

    assert!(!input_bytes.is_empty(), "source must not be truncated when out is a hardlink");
    assert_eq!(input_bytes, link_bytes, "hardlinked input and out must remain identical");
    assert!(
        String::from_utf8_lossy(&input_bytes).contains("id,name,active,score"),
        "output should be TOON, not the original JSON"
    );
}
