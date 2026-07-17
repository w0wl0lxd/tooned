// SPDX-License-Identifier: AGPL-3.0-only

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
fn convert_to_json_decodes_toon_whose_first_key_is_class() {
    // Regression for the `class `/`!schema ` prefix ambiguity: a plain TOON
    // document whose first key is `class` must decode as TOON (not be
    // mistaken for a TRON record header and fail with exit 3).
    let dir = tempfile::tempdir().expect("tempdir");
    let toon = "class: \"user\"\nid: 7\nname: \"Ada\"\n";
    let path = write_fixture(&dir, "input.toon", toon);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(r#""class":"user""#)
                .and(predicate::str::contains(r#""id":7"#))
                .and(predicate::str::contains(r#""name":"Ada""#)),
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

#[test]
fn convert_to_tron_writes_tron_content_to_stdout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json = uniform_array_json(20);
    let path = write_fixture(&dir, "input.json", &json);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "tron"])
        .assert()
        .success()
        // TRON encodes the schema once as a class definition and each record
        // as a compact `A(...)` instantiation.
        .stdout(predicate::str::contains("class A:"))
        .stdout(predicate::str::contains("A(0,\"row-0\",true,0.5)"));
}

#[test]
fn convert_to_json_decodes_a_tron_file_back_to_compact_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let tron =
        "class A: id, name, active, score\n\n[A(0,\"row-0\",true,0.5),A(1,\"row-1\",false,1.5)]";
    let path = write_fixture(&dir, "input.tron", tron);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(r#""id":0"#)
                .and(predicate::str::contains(r#""name":"row-0"#))
                .and(predicate::str::contains(r#""active":true"#))
                .and(predicate::str::contains(r#""score":0.5"#)),
        );
}

#[test]
fn convert_to_tron_on_ndjson_with_format_hint_produces_tron_stream() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ndjson = "{\"id\":0,\"name\":\"row-0\",\"active\":true,\"score\":0.5}\n{\"id\":1,\"name\":\"row-1\",\"active\":false,\"score\":1.5}\n";
    let path = write_fixture(&dir, "input.json", ndjson);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args([
            "convert",
            path.to_str().expect("utf8 path"),
            "--to",
            "tron",
            "--format-hint",
            "ndjson",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("class A:"))
        .stdout(predicate::str::contains("A(0,\"row-0\",true,0.5)"));
}

#[test]
fn convert_to_tron_on_ndjson_extension_produces_tron_stream() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ndjson = "{\"id\":0,\"name\":\"row-0\",\"active\":true,\"score\":0.5}\n{\"id\":1,\"name\":\"row-1\",\"active\":false,\"score\":1.5}\n";
    let path = write_fixture(&dir, "input.ndjson", ndjson);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "tron"])
        .assert()
        .success()
        .stdout(predicate::str::contains("class A:"))
        .stdout(predicate::str::contains("A(0,\"row-0\",true,0.5)"));
}

#[test]
fn convert_to_tron_on_jsonl_extension_produces_tron_stream() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ndjson = "{\"id\":0,\"name\":\"row-0\",\"active\":true,\"score\":0.5}\n{\"id\":1,\"name\":\"row-1\",\"active\":false,\"score\":1.5}\n";
    let path = write_fixture(&dir, "input.jsonl", ndjson);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "tron"])
        .assert()
        .success()
        .stdout(predicate::str::contains("class A:"))
        .stdout(predicate::str::contains("A(0,\"row-0\",true,0.5)"));
}

#[test]
fn convert_to_tron_round_trips_ndjson_via_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ndjson = "{\"id\":0,\"name\":\"row-0\",\"active\":true,\"score\":0.5}\n{\"id\":1,\"name\":\"row-1\",\"active\":false,\"score\":1.5}\n";
    let input_path = write_fixture(&dir, "input.ndjson", ndjson);
    let tron_path = dir.path().join("output.tron");
    let json_path = dir.path().join("output.json");

    // Convert NDJSON to TRON
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args([
            "convert",
            input_path.to_str().expect("utf8 path"),
            "--to",
            "tron",
            "--out",
            tron_path.to_str().expect("utf8 path"),
        ])
        .assert()
        .success();

    // Convert TRON back to JSON
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args([
            "convert",
            tron_path.to_str().expect("utf8 path"),
            "--to",
            "json",
            "--out",
            json_path.to_str().expect("utf8 path"),
        ])
        .assert()
        .success();

    // Read and compare
    let original_json = std::fs::read_to_string(&input_path).expect("read original");
    let round_trip_json = std::fs::read_to_string(&json_path).expect("read round-trip");

    // Parse both as JSON arrays to compare semantically (whitespace may differ)
    let original_lines: Vec<&str> = original_json.lines().filter(|l| !l.is_empty()).collect();
    let original_array_str = format!("[{}]", original_lines.join(","));
    let original_value: serde_json::Value =
        serde_json::from_str(&original_array_str).expect("parse original");
    let round_trip_value: serde_json::Value =
        serde_json::from_str(&round_trip_json).expect("parse round-trip");

    assert_eq!(original_value, round_trip_value, "round-trip should preserve data");
}

#[test]
fn large_ndjson_file_converts_with_to_tron() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut ndjson = String::new();
    for i in 0..10000 {
        let _ = writeln!(
            ndjson,
            "{{\"id\":{},\"name\":\"row-{}\",\"active\":{},\"score\":{}}}",
            i,
            i,
            i % 2 == 0,
            f64::from(i) + 0.5
        );
    }
    let path = write_fixture(&dir, "large.ndjson", &ndjson);

    // This file is > 2 MiB (default max_input_bytes), so it should use streaming
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "tron"])
        .assert()
        .success()
        .stdout(predicate::str::contains("class A:"))
        .stdout(predicate::str::contains("A(0,\"row-0\",true,0.5)"));
}

#[test]
fn adaptive_bounded_chooses_toon_for_small_ndjson() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut ndjson = String::new();
    for i in 0..1000 {
        let _ = writeln!(
            ndjson,
            "{{\"id\":{},\"name\":\"row-{}\",\"active\":{},\"score\":{}}}",
            i,
            i,
            i % 2 == 0,
            f64::from(i) + 0.5
        );
    }
    let path = write_fixture(&dir, "input.ndjson", &ndjson);

    // Small NDJSON fits in memory, so the default adaptive path routes through
    // maybe_tooned and picks TOON when it beats compact JSON.
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("[1000]{"));
}

#[test]
fn adaptive_bounded_chooses_toon_for_single_row_ndjson() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Single object - TOON still wins for a uniform flat object.
    let ndjson = "{\"id\":0,\"name\":\"row-0\",\"active\":true,\"score\":0.5}\n";
    let path = write_fixture(&dir, "input.ndjson", ndjson);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("[1]{"));
}

#[allow(clippy::expect_used)]
fn write_bytes_fixture(dir: &tempfile::TempDir, name: &str, contents: &[u8]) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let mut f = fs::File::create(&path).expect("create fixture file");
    f.write_all(contents).expect("write fixture file");
    path
}

#[test]
fn convert_to_toon_on_msgpack_extension_produces_toon() {
    let dir = tempfile::tempdir().expect("tempdir");
    // MessagePack array of two uniform objects: [{"id":1,"name":"x"}, {"id":2,"name":"y"}]
    let msgpack = [
        0x92, 0x82, 0xa2, 0x69, 0x64, 0x01, 0xa4, 0x6e, 0x61, 0x6d, 0x65, 0xa1, 0x78, 0x82, 0xa2,
        0x69, 0x64, 0x02, 0xa4, 0x6e, 0x61, 0x6d, 0x65, 0xa1, 0x79,
    ];
    let path = write_bytes_fixture(&dir, "input.msgpack", &msgpack);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "toon"])
        .assert()
        .success()
        .stdout(predicate::str::contains("{id,name}"))
        .stdout(predicate::str::contains("1,x"))
        .stdout(predicate::str::contains("2,y"));
}

#[test]
fn convert_to_toon_on_cbor_extension_produces_toon() {
    let dir = tempfile::tempdir().expect("tempdir");
    // CBOR array of two uniform objects: [{"id":1,"name":"x"}, {"id":2,"name":"y"}]
    let cbor = [
        0x82, 0xa2, 0x62, 0x69, 0x64, 0x01, 0x64, 0x6e, 0x61, 0x6d, 0x65, 0x61, 0x78, 0xa2, 0x62,
        0x69, 0x64, 0x02, 0x64, 0x6e, 0x61, 0x6d, 0x65, 0x61, 0x79,
    ];
    let path = write_bytes_fixture(&dir, "input.cbor", &cbor);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "toon"])
        .assert()
        .success()
        .stdout(predicate::str::contains("{id,name}"))
        .stdout(predicate::str::contains("1,x"))
        .stdout(predicate::str::contains("2,y"));
}

#[test]
fn convert_to_toon_on_json5_extension_produces_toon() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json5 = "[{a:1,b:2},{a:3,b:4}]";
    let path = write_fixture(&dir, "input.json5", json5);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "toon"])
        .assert()
        .success()
        .stdout(predicate::str::contains("{a,b}"))
        .stdout(predicate::str::contains("1,2"))
        .stdout(predicate::str::contains("3,4"));
}

#[test]
fn convert_config_format_hint_overrides_extension() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Extension would map to JSON, but the configured default should win.
    write_fixture(&dir, ".tooned.toml", "format_hint = \"json5\"\n");
    let path = write_fixture(&dir, "input.json", "[{a:1,b:2},{a:3,b:4}]");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["convert", path.to_str().expect("utf8 path"), "--to", "toon"])
        .assert()
        .success()
        .stdout(predicate::str::contains("{a,b}"))
        .stdout(predicate::str::contains("1,2"))
        .stdout(predicate::str::contains("3,4"));
}
