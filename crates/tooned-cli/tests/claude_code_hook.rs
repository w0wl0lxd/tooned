//! Integration tests for `tooned hook run --claude-code` (T024, T026).
//! See `specs/001-adaptive-toon-conversion/contracts/claude-code-hook.md`.

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

/// Builds a `PostToolUse` stdin payload matching the exact shape documented
/// in `contracts/claude-code-hook.md` (`tool_output` is the tool's raw
/// result text, carried as a JSON string field).
fn post_tool_use_payload(tool_output: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "cat data.json"},
        "tool_output": tool_output,
        "session_id": "test-session",
        "cwd": "/tmp",
    })
    .to_string()
}

#[test]
fn convertible_tool_output_prints_hook_specific_output_and_exits_0() {
    let tool_output = uniform_array_json(20);
    let stdin = post_tool_use_payload(&tool_output);

    let assert = Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--claude-code"])
        .write_stdin(stdin)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
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

    let event_name = parsed
        .get("hookSpecificOutput")
        .and_then(|v| v.get("hookEventName"))
        .and_then(serde_json::Value::as_str);
    assert_eq!(event_name, Some("PostToolUse"));
}

#[test]
fn non_json_tool_output_produces_no_stdout_and_exits_0() {
    let stdin = post_tool_use_payload("just some prose, nothing structured here");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--claude-code"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn malformed_stdin_json_produces_no_stdout_and_exits_0() {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--claude-code"])
        .write_stdin("{ this is not valid JSON at all")
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn oversized_tool_output_produces_no_stdout_and_exits_0() {
    // Comfortably larger than the default 2 MiB max_input_bytes cap.
    let huge = uniform_array_json(200_000);
    let stdin = post_tool_use_payload(&huge);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--claude-code"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn missing_tool_output_field_produces_no_stdout_and_exits_0() {
    let stdin = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {},
    })
    .to_string();

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--claude-code"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn empty_stdin_produces_no_stdout_and_exits_0() {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--claude-code"])
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::eq(""));
}
