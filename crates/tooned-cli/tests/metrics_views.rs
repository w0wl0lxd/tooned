// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


//! Integration tests for the local metrics ledger and the tooned heatmap /
//! tooned metrics views. All reads/writes are scoped to a unique temp dir via
//! TOONED_METRICS_DIR (see store::user_global_db_path), so tests never touch a
//! real user ledger.

use assert_cmd::Command;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn tmp_metrics_dir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = env::temp_dir().join(format!("tooned-metrics-it-{}-{}", std::process::id(), n));
    fs::create_dir_all(&dir).ok();
    dir
}

fn cmd_with(dir: &PathBuf) -> Command {
    let mut cmd = Command::cargo_bin("tooned").unwrap();
    cmd.env("TOONED_METRICS_DIR", dir);
    cmd
}

fn record_one_event(dir: &PathBuf) {
    let mut cmd = cmd_with(dir);
    cmd.args(["pipe"]);
    cmd.write_stdin(r#"{"hello":"world","n":123}"#);
    cmd.assert().success();
}

#[test]
fn summary_records_events() {
    let dir = tmp_metrics_dir();
    record_one_event(&dir);
    let mut cmd = cmd_with(&dir);
    cmd.args(["metrics", "--global", "summary"]);
    let out = cmd.output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("tooned metrics -- summary"), "summary header missing: {s}");
    assert!(s.contains("passthroughs:"), "summary missing passthroughs: {s}");
}

#[test]
fn heatmap_global_renders() {
    let dir = tmp_metrics_dir();
    record_one_event(&dir);
    let mut cmd = cmd_with(&dir);
    cmd.args(["heatmap", "--global"]);
    let out = cmd.output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("tokens saved"), "heatmap missing header: {s}");
}

#[test]
fn breakdown_lists_surfaces() {
    let dir = tmp_metrics_dir();
    record_one_event(&dir);
    let mut cmd = cmd_with(&dir);
    cmd.args(["metrics", "--global", "breakdown"]);
    let out = cmd.output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.to_lowercase().contains("surface"), "breakdown missing surface label: {s}");
}

#[test]
fn reset_clears_ledger() {
    let dir = tmp_metrics_dir();
    record_one_event(&dir);
    let mut cmd = cmd_with(&dir);
    cmd.args(["metrics", "--global", "reset", "--yes"]);
    let out = cmd.output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("reset ledger"), "reset missing confirmation: {s}");
    let mut cmd2 = cmd_with(&dir);
    cmd2.args(["metrics", "--global", "summary"]);
    let out2 = cmd2.output().unwrap();
    let s2 = String::from_utf8_lossy(&out2.stdout);
    assert!(
        s2.contains("no metrics recorded yet") || s2.contains("total saved:    0 tokens"),
        "ledger not cleared: {s2}"
    );
}

#[test]
fn project_scope_clean_when_empty() {
    let dir = tmp_metrics_dir();
    let mut cmd = cmd_with(&dir);
    cmd.current_dir(&dir);
    cmd.args(["heatmap"]);
    let out = cmd.output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("no metrics recorded yet") || s.contains("tokens saved"), "unexpected: {s}");
}
