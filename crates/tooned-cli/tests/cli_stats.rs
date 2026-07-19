// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


//! Contract test for `tooned stats [path] [--top N]` (T053).
//! See `specs/001-adaptive-toon-conversion/contracts/cli.md`.

use std::fmt::Write as _;
use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

/// Builds a uniform array of `rows` objects, each with `field_count` short
/// numeric fields (`f0`, `f1`, ...) -- more fields means more repeated-key
/// structural overhead in JSON that TOON's tabular header-once encoding
/// eliminates, so a bigger `field_count` reliably produces a bigger
/// `savings_pct` (TOON only wins on the *structural* overhead; using large
/// string values instead would do the opposite, since a big shared payload
/// dilutes that fixed per-row structural saving down to a small
/// percentage of the total).
fn uniform_array_json(rows: usize, field_count: usize) -> String {
    let mut s = String::from("[");
    for i in 0..rows {
        if i > 0 {
            s.push(',');
        }
        s.push('{');
        for f in 0..field_count {
            if f > 0 {
                s.push(',');
            }
            let _ = write!(s, r#""f{f}":{i}"#);
        }
        s.push('}');
    }
    s.push(']');
    s
}

fn run_index(dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("tooned")?.current_dir(dir).arg("index").assert().success();
    Ok(())
}

#[test]
fn stats_without_an_existing_index_fails_gracefully_with_exit_code_1() {
    let dir = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .arg("stats")
        .assert()
        .failure()
        .code(predicate::eq(1))
        .stderr(predicate::str::contains("index"));
}

#[test]
fn stats_orders_results_by_savings_pct_descending() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Distinctly different savings potential per file: more repeated-key
    // structural overhead compresses relatively better under TOON.
    fs::write(dir.path().join("low.json"), uniform_array_json(20, 2)).expect("write low.json");
    fs::write(dir.path().join("mid.json"), uniform_array_json(20, 10)).expect("write mid.json");
    fs::write(dir.path().join("high.json"), uniform_array_json(20, 30)).expect("write high.json");
    run_index(dir.path()).expect("run_index");

    let output = Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .arg("stats")
        .output()
        .expect("run stats");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    let pos_high = stdout.find("high.json").expect("high.json present in stats output");
    let pos_mid = stdout.find("mid.json").expect("mid.json present in stats output");
    let pos_low = stdout.find("low.json").expect("low.json present in stats output");
    assert!(
        pos_high < pos_mid,
        "highest savings must be listed before mid savings, got:\n{stdout}"
    );
    assert!(pos_mid < pos_low, "mid savings must be listed before lowest savings, got:\n{stdout}");
}

#[test]
fn stats_top_n_limits_the_result_count() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("a.json"), uniform_array_json(20, 2)).expect("write a.json");
    fs::write(dir.path().join("b.json"), uniform_array_json(20, 10)).expect("write b.json");
    fs::write(dir.path().join("c.json"), uniform_array_json(20, 30)).expect("write c.json");
    run_index(dir.path()).expect("run_index");

    let output = Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["stats", "--top", "1"])
        .output()
        .expect("run stats --top 1");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    let mentioned =
        ["a.json", "b.json", "c.json"].iter().filter(|name| stdout.contains(**name)).count();
    assert_eq!(mentioned, 1, "--top 1 must limit output to exactly one file, got:\n{stdout}");
    assert!(stdout.contains("c.json"), "the single result must be the highest-savings file");
}

#[test]
fn stats_json_output_is_valid_and_ordered_by_savings_descending() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("low.json"), uniform_array_json(20, 2)).expect("write low.json");
    fs::write(dir.path().join("mid.json"), uniform_array_json(20, 10)).expect("write mid.json");
    fs::write(dir.path().join("high.json"), uniform_array_json(20, 30)).expect("write high.json");
    run_index(dir.path()).expect("run_index");

    let output = Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["stats", "--json"])
        .output()
        .expect("run stats --json");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    let parsed: Vec<Value> = serde_json::from_str(&stdout).expect("stats --json emits valid JSON");
    assert_eq!(parsed.len(), 3, "expected one JSON object per indexed file");

    let first = parsed.first().expect("first entry");
    let second = parsed.get(1).expect("second entry");
    let third = parsed.get(2).expect("third entry");
    assert_eq!(first.get("path").expect("path").as_str().expect("path string"), "high.json");
    assert_eq!(second.get("path").expect("path").as_str().expect("path string"), "mid.json");
    assert_eq!(third.get("path").expect("path").as_str().expect("path string"), "low.json");
    assert!(
        first.get("savings_pct").expect("savings").as_f64().expect("savings number")
            > second.get("savings_pct").expect("savings").as_f64().expect("savings number")
    );
    assert!(
        second.get("savings_pct").expect("savings").as_f64().expect("savings number")
            > third.get("savings_pct").expect("savings").as_f64().expect("savings number")
    );
}

#[test]
fn stats_json_top_n_limits_result_count() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("a.json"), uniform_array_json(20, 2)).expect("write a.json");
    fs::write(dir.path().join("b.json"), uniform_array_json(20, 10)).expect("write b.json");
    fs::write(dir.path().join("c.json"), uniform_array_json(20, 30)).expect("write c.json");
    run_index(dir.path()).expect("run_index");

    let output = Command::cargo_bin("tooned")
        .expect("binary exists")
        .current_dir(dir.path())
        .args(["stats", "--json", "--top", "1"])
        .output()
        .expect("run stats --json --top 1");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    let parsed: Vec<Value> =
        serde_json::from_str(&stdout).expect("stats --json --top 1 emits valid JSON");
    assert_eq!(parsed.len(), 1);
    assert_eq!(
        parsed
            .first()
            .expect("one entry")
            .get("path")
            .expect("path field")
            .as_str()
            .expect("path string"),
        "c.json"
    );
}
