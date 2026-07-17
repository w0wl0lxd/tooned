// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Contract tests for `tooned_index_build`/`tooned_index_refresh`/
//! `tooned_stats` (T074). See
//! `specs/001-adaptive-toon-conversion/contracts/mcp-tools.md`.
//!
//! Drives the real `tooned mcp serve` binary over its actual stdio JSON-RPC
//! transport (`tests/common/mcp_client.rs`).

mod common;

use std::fmt::Write as _;
use std::fs;
use std::time::{Duration, SystemTime};

use common::mcp_client::McpClient;
use serde_json::{Value, json};

/// `Value` indexing (`v["key"]`) is off-limits under `clippy::indexing_slicing`
/// (denied workspace-wide, including tests); every field access below goes
/// through explicit `.get(...)` instead.
#[allow(clippy::expect_used)] // test-only helper in an integration-test binary, not `cfg(test)`-scoped
fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).expect("expected field")
}

fn is_tool_error(result: &Value) -> bool {
    result.get("isError").and_then(Value::as_bool) == Some(true)
}

/// Bumps a file's mtime forward, simulating a real edit -- `sync`'s stat-
/// first logic skips re-hashing entirely when mtime is unchanged, and a
/// same-second write/rewrite pair in a fast test can otherwise land on an
/// unchanged mtime at the filesystem's timestamp resolution (mirrors
/// `crates/tooned-index/tests/sync.rs`'s `set_mtime` helper).
fn set_mtime(path: &std::path::Path, when: SystemTime) -> std::io::Result<()> {
    // Open with write permission; Windows requires a writable handle to call
    // `SetFileTime` via `set_modified`.
    let file = fs::OpenOptions::new().write(true).open(path)?;
    file.set_modified(when)
}

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

#[test]
fn tooned_index_build_scans_a_project_and_updates_gitignore_only_on_first_build() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("a.json"), uniform_array_json(20, 5)).expect("write a.json");
    fs::write(dir.path().join("b.json"), uniform_array_json(20, 5)).expect("write b.json");

    let mut client = McpClient::spawn();
    let path = dir.path().to_str().expect("utf8 tempdir path").to_string();

    let first = client.call_tool("tooned_index_build", &json!({ "path": path }));
    let first_result = field(&first, "result");
    assert!(!is_tool_error(first_result));
    let first_structured = field(first_result, "structuredContent");
    assert_eq!(field(first_structured, "files_scanned"), &json!(2));
    assert_eq!(field(first_structured, "gitignore_updated"), &json!(true));

    let gitignore =
        fs::read_to_string(dir.path().join(".gitignore")).expect("read .gitignore after build");
    assert!(gitignore.contains(".tooned/"));

    // A second build against the same (already-indexed) project must not
    // report a fresh gitignore update (data-model.md: append only happens
    // on first creation).
    let second = client.call_tool("tooned_index_build", &json!({ "path": path }));
    let second_structured = field(field(&second, "result"), "structuredContent");
    assert_eq!(field(second_structured, "gitignore_updated"), &json!(false));
}

#[test]
fn tooned_index_build_reports_a_tool_level_error_for_a_missing_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("does-not-exist");

    let mut client = McpClient::spawn();
    let response = client
        .call_tool("tooned_index_build", &json!({ "path": missing.to_str().expect("utf8 path") }));
    let result = response
        .get("result")
        .expect("a missing path must be a tools/call `result` with isError, never a server crash");
    assert!(is_tool_error(result));
}

#[test]
fn tooned_index_refresh_rescans_changed_files_and_prunes_deleted_ones() {
    let dir = tempfile::tempdir().expect("tempdir");
    let a_path = dir.path().join("a.json");
    let b_path = dir.path().join("b.json");
    fs::write(&a_path, uniform_array_json(20, 5)).expect("write a.json");
    fs::write(&b_path, uniform_array_json(20, 5)).expect("write b.json");

    let mut client = McpClient::spawn();
    let path = dir.path().to_str().expect("utf8 tempdir path").to_string();
    client.call_tool("tooned_index_build", &json!({ "path": &path }));

    // Change one file's content, delete the other. Bump the edited file's
    // mtime forward so `sync`'s stat-first logic can't skip it as
    // apparently-unchanged at the filesystem's timestamp resolution.
    fs::write(&a_path, uniform_array_json(25, 6)).expect("rewrite a.json");
    let future = SystemTime::now() + Duration::from_mins(2);
    set_mtime(&a_path, future).expect("set_mtime");
    fs::remove_file(&b_path).expect("remove b.json");

    let refresh = client.call_tool("tooned_index_refresh", &json!({ "path": path }));
    let structured = field(field(&refresh, "result"), "structuredContent");
    let files_rescanned = field(structured, "files_rescanned").as_u64().expect("files_rescanned");
    assert!(
        files_rescanned >= 1,
        "the edited file must be counted as rescanned, got: {structured}"
    );
    assert_eq!(field(structured, "files_pruned"), &json!(1));
}

#[test]
fn tooned_stats_returns_results_ordered_by_savings_pct_descending() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("low.json"), uniform_array_json(20, 2)).expect("write low.json");
    fs::write(dir.path().join("high.json"), uniform_array_json(20, 30)).expect("write high.json");

    let mut client = McpClient::spawn();
    let path = dir.path().to_str().expect("utf8 tempdir path").to_string();
    client.call_tool("tooned_index_build", &json!({ "path": &path }));

    let response = client.call_tool("tooned_stats", &json!({ "path": path }));
    let structured = field(field(&response, "result"), "structuredContent");
    let results = field(structured, "results").as_array().expect("results array");
    assert_eq!(results.len(), 2);
    let paths: Vec<&str> =
        results.iter().map(|r| field(r, "path").as_str().expect("path field")).collect();
    assert!(
        paths.iter().position(|p| p.contains("high.json"))
            < paths.iter().position(|p| p.contains("low.json")),
        "expected high.json before low.json, got: {paths:?}"
    );
}

#[test]
fn tooned_stats_top_n_limits_the_result_count() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("a.json"), uniform_array_json(20, 2)).expect("write a.json");
    fs::write(dir.path().join("b.json"), uniform_array_json(20, 10)).expect("write b.json");
    fs::write(dir.path().join("c.json"), uniform_array_json(20, 30)).expect("write c.json");

    let mut client = McpClient::spawn();
    let path = dir.path().to_str().expect("utf8 tempdir path").to_string();
    client.call_tool("tooned_index_build", &json!({ "path": &path }));

    let response = client.call_tool("tooned_stats", &json!({ "path": path, "top_n": 1 }));
    let structured = field(field(&response, "result"), "structuredContent");
    let results = field(structured, "results").as_array().expect("results array");
    assert_eq!(results.len(), 1);
}

#[test]
fn tooned_stats_reports_a_tool_level_error_when_no_index_exists() {
    let dir = tempfile::tempdir().expect("tempdir");

    let mut client = McpClient::spawn();
    let response = client.call_tool(
        "tooned_stats",
        &json!({ "path": dir.path().to_str().expect("utf8 tempdir path") }),
    );
    let result = response.get("result").expect(
        "no existing index must be a tools/call `result` with isError, never a server crash",
    );
    assert!(is_tool_error(result));
}
