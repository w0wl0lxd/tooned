// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the plugin-wrapped agent hooks (OpenCode, Kilo, Pi).
//!
//! These agents do not call `tooned` directly; their generated plugin files call
//! `tooned hook run --<flag>` with a Claude-compatible `tool_output` payload.
//! The runtime path is therefore identical to `--claude-code` and is tested here
//! for each wrapper flag.

use std::fmt::Write as _;

use assert_cmd::Command;
use assert_cmd::assert::Assert;
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

fn run_with_flag(agent: &str, payload: &str) -> Assert {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", agent])
        .write_stdin(payload)
        .assert()
        .success()
}

fn convertible_payload() -> String {
    serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "data.json" },
        "tool_output": uniform_array_json(20),
    })
    .to_string()
}

fn passthrough_payload() -> String {
    serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "notes.txt" },
        "tool_output": "just some prose, nothing structured here",
    })
    .to_string()
}

#[test]
fn opencode_convertible_tool_output_prints_updated_tool_output() {
    let output = run_with_flag("--opencode", &convertible_payload()).get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON, got {stdout:?}: {e}"));
    let updated = parsed
        .get("hookSpecificOutput")
        .and_then(|v| v.get("updatedToolOutput"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("expected hookSpecificOutput.updatedToolOutput, got {parsed}"));
    assert!(
        updated.contains("id,name,active,score"),
        "expected TOON tabular header in updatedToolOutput, got {updated:?}"
    );
}

#[test]
fn opencode_non_json_tool_output_passthrough() {
    run_with_flag("--opencode", &passthrough_payload()).stdout(predicate::eq(""));
}

#[test]
fn kilo_convertible_tool_output_prints_updated_tool_output() {
    let output = run_with_flag("--kilo", &convertible_payload()).get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON, got {stdout:?}: {e}"));
    let updated = parsed
        .get("hookSpecificOutput")
        .and_then(|v| v.get("updatedToolOutput"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("expected hookSpecificOutput.updatedToolOutput, got {parsed}"));
    assert!(
        updated.contains("id,name,active,score"),
        "expected TOON tabular header in updatedToolOutput, got {updated:?}"
    );
}

#[test]
fn kilo_non_json_tool_output_passthrough() {
    run_with_flag("--kilo", &passthrough_payload()).stdout(predicate::eq(""));
}

#[test]
fn pi_convertible_tool_output_prints_updated_tool_output() {
    let output = run_with_flag("--pi", &convertible_payload()).get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON, got {stdout:?}: {e}"));
    let updated = parsed
        .get("hookSpecificOutput")
        .and_then(|v| v.get("updatedToolOutput"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("expected hookSpecificOutput.updatedToolOutput, got {parsed}"));
    assert!(
        updated.contains("id,name,active,score"),
        "expected TOON tabular header in updatedToolOutput, got {updated:?}"
    );
}

#[test]
fn pi_non_json_tool_output_passthrough() {
    run_with_flag("--pi", &passthrough_payload()).stdout(predicate::eq(""));
}
