// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for `tooned hook run --devin`.
//! Devin CLI `PostToolUse` stdin carries the tool's raw output under
//! `tool_response.output` (an object with `success`, `output`, `error`).
//! See <https://docs.devin.ai/cli/extensibility/hooks/lifecycle-hooks>.

use std::fmt::Write as _;

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

fn post_tool_use_payload(tool_response: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "exec",
        "tool_input": { "command": "cat data.json" },
        "tool_response": {
            "success": true,
            "output": tool_response,
            "error": null,
        },
    })
    .to_string()
}

#[test]
fn convertible_tool_response_prints_hook_specific_output_and_exits_0() {
    let tool_response = uniform_array_json(20);
    let stdin = post_tool_use_payload(&tool_response);

    let assert = Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--devin"])
        .write_stdin(stdin)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON, got {stdout:?}: {e}"));

    let additional_context = parsed
        .get("hookSpecificOutput")
        .and_then(|v| v.get("additionalContext"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("expected hookSpecificOutput.additionalContext, got {parsed}"));

    assert!(
        additional_context.contains("id,name,active,score"),
        "expected TOON tabular header in additionalContext, got {additional_context:?}"
    );
}

#[test]
fn non_json_tool_response_produces_no_stdout_and_exits_0() {
    let stdin = post_tool_use_payload("just some prose, nothing structured here");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--devin"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn malformed_stdin_json_produces_no_stdout_and_exits_0() {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--devin"])
        .write_stdin("{ this is not valid JSON at all")
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn empty_stdin_produces_no_stdout_and_exits_0() {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--devin"])
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn tool_response_as_string_also_works() {
    // Defensive: if Devin ever passes `tool_response` as a plain string,
    // the hook should still process it like Codex does.
    let stdin = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "exec",
        "tool_input": { "command": "cat data.json" },
        "tool_response": uniform_array_json(20),
    })
    .to_string();

    let assert = Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--devin"])
        .write_stdin(stdin)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "expected hook output for string tool_response");
}
