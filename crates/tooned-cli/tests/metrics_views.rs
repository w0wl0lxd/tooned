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
    let mut cmd = Command::cargo_bin("tooned").expect("binary exists");
    cmd.env("TOONED_METRICS_DIR", dir);
    cmd
}

/// Runs a command and returns stdout, keeping stderr for the failure message.
/// `assert_cmd` hides stderr by default, which turned every ledger problem
/// into an assertion on an empty string with no cause attached.
fn run(cmd: &mut Command, what: &str) -> (String, String) {
    let out = cmd.output().unwrap_or_else(|err| panic!("run {what}: {err}"));
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `pipe` reports metrics on a best-effort basis: a ledger it cannot open is
/// swallowed and the process still exits 0. Asserting only on the exit status
/// therefore proves nothing, so this checks that the ledger really exists.
fn record_one_event(dir: &PathBuf) {
    let mut cmd = cmd_with(dir);
    cmd.args(["pipe"]);
    cmd.write_stdin(r#"{"hello":"world","n":123}"#);
    let (_, stderr) = run(&mut cmd, "pipe");
    let db = dir.join("metrics.db");
    assert!(
        db.exists(),
        "pipe recorded no ledger at {}: stderr {stderr}, dir contains {:?}",
        db.display(),
        fs::read_dir(dir).map(|entries| entries
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.file_name())
            .collect::<Vec<_>>())
    );
}

#[test]
fn summary_records_events() {
    let dir = tmp_metrics_dir();
    record_one_event(&dir);
    let mut cmd = cmd_with(&dir);
    cmd.args(["metrics", "--global", "summary"]);
    let (s, e) = run(&mut cmd, "metrics summary");
    assert!(s.contains("tooned metrics -- summary"), "summary header missing: {s}, stderr {e}");
    assert!(s.contains("passthroughs:"), "summary missing passthroughs: {s}, stderr {e}");
}

#[test]
fn heatmap_global_renders() {
    let dir = tmp_metrics_dir();
    record_one_event(&dir);
    let mut cmd = cmd_with(&dir);
    cmd.args(["heatmap", "--global"]);
    let (s, e) = run(&mut cmd, "heatmap global");
    assert!(s.contains("tokens saved"), "heatmap missing header: {s}, stderr {e}");
}

#[test]
fn breakdown_lists_surfaces() {
    let dir = tmp_metrics_dir();
    record_one_event(&dir);
    let mut cmd = cmd_with(&dir);
    cmd.args(["metrics", "--global", "breakdown"]);
    let (s, e) = run(&mut cmd, "breakdown");
    assert!(
        s.to_lowercase().contains("surface"),
        "breakdown missing surface label: {s}, stderr {e}"
    );
}

#[test]
fn reset_clears_ledger() {
    let dir = tmp_metrics_dir();
    record_one_event(&dir);
    let mut cmd = cmd_with(&dir);
    cmd.args(["metrics", "--global", "reset", "--yes"]);
    let (s, e) = run(&mut cmd, "reset");
    assert!(s.contains("reset ledger"), "reset missing confirmation: {s}, stderr {e}");
    let mut cmd2 = cmd_with(&dir);
    cmd2.args(["metrics", "--global", "summary"]);
    let (s2, e2) = run(&mut cmd2, "summary after reset");
    assert!(
        s2.contains("no metrics recorded yet") || s2.contains("total saved:    0 tokens"),
        "ledger not cleared: {s2}, stderr {e2}"
    );
}

#[test]
fn project_scope_clean_when_empty() {
    let dir = tmp_metrics_dir();
    let mut cmd = cmd_with(&dir);
    cmd.current_dir(&dir);
    cmd.args(["heatmap"]);
    let (s, e) = run(&mut cmd, "project heatmap");
    assert!(
        s.contains("no metrics recorded yet") || s.contains("tokens saved"),
        "unexpected: {s}, stderr {e}"
    );
}
