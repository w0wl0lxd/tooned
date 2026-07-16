//! Integration tests for `tooned hook run --codex` (T025, T026).
//! See `specs/001-adaptive-toon-conversion/contracts/codex-hook.md`.

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

/// Real Codex CLI `PostToolUse` stdin shape, verified against
/// `openai/codex`'s `codex-rs/hooks/src/events/post_tool_use.rs`: the tool's
/// raw result text is carried in a field named `tool_response` -- NOT
/// `tool_output` (that's Claude Code's field name only; see
/// `contracts/codex-hook.md`'s I/O contract section).
fn post_tool_use_payload(tool_response: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "cat data.json"},
        "tool_response": tool_response,
    })
    .to_string()
}

#[test]
fn convertible_tool_response_prints_hook_specific_output_and_exits_0() {
    let tool_response = uniform_array_json(20);
    let stdin = post_tool_use_payload(&tool_response);

    let assert = Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--codex"])
        .write_stdin(stdin)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON, got {stdout:?}: {e}"));

    // Codex's real output parser has no `updatedToolOutput` field (unlike
    // Claude Code) -- only `hookSpecificOutput.additionalContext` is
    // recognized for surfacing extra content.
    let updated = parsed
        .get("hookSpecificOutput")
        .and_then(|v| v.get("additionalContext"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("expected hookSpecificOutput.additionalContext, got {parsed}"));

    assert!(
        updated.contains("id,name,active,score"),
        "expected TOON tabular header in additionalContext, got {updated:?}"
    );
}

#[test]
fn non_json_tool_response_produces_no_stdout_and_exits_0() {
    let stdin = post_tool_use_payload("just some prose, nothing structured here");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--codex"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn malformed_stdin_json_produces_no_stdout_and_exits_0() {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--codex"])
        .write_stdin("{ this is not valid JSON at all")
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn oversized_tool_response_produces_no_stdout_and_exits_0() {
    let huge = uniform_array_json(200_000);
    let stdin = post_tool_use_payload(&huge);

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--codex"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn empty_stdin_produces_no_stdout_and_exits_0() {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--codex"])
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn tool_output_field_is_not_recognized_by_codex_and_produces_no_stdout() {
    // Regression test: a payload using Claude Code's `tool_output` field
    // name (rather than Codex's real `tool_response`) must not be picked up
    // by the Codex hook -- confirms the two protocols are not accidentally
    // sharing a stdin field name again.
    let stdin = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "cat data.json"},
        "tool_output": uniform_array_json(20),
    })
    .to_string();

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--codex"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
}
