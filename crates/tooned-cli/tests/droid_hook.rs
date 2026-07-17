// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Integration tests for `tooned hook run --droid`.
//! Droid `PostToolUse` stdin carries `tool_response` as either a raw string
//! or an object whose schema is tool-specific; `hooks/mod.rs` extracts
//! common string fields (`output`, `content`, `stdout`, `result`, `text`)
//! and MCP-style `content` arrays. See <https://docs.factory.ai/reference/hooks-reference>.

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

fn object_payload(output: &serde_json::Value) -> String {
    serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Execute",
        "tool_input": { "command": "cat data.json" },
        "tool_response": output,
    })
    .to_string()
}

#[test]
fn convertible_output_field_prints_hook_specific_output_and_exits_0() {
    let tool_response = uniform_array_json(20);
    let stdin = object_payload(&serde_json::json!({
        "success": true,
        "output": tool_response,
        "error": null,
    }));

    let assert = Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--droid"])
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
fn string_tool_response_also_works() {
    let stdin = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Execute",
        "tool_input": { "command": "cat data.json" },
        "tool_response": uniform_array_json(20),
    })
    .to_string();

    let assert = Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--droid"])
        .write_stdin(stdin)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "expected hook output for string tool_response");
}

#[test]
fn content_array_extracts_text_items() {
    let stdin = object_payload(&serde_json::json!({
        "content": [
            { "type": "text", "text": "first" },
            { "type": "text", "text": uniform_array_json(20) },
        ],
    }));

    // Concatenation with a non-JSON prefix should passthrough, so no stdout.
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--droid"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn content_array_with_only_json_text_item_converts() {
    let stdin = object_payload(&serde_json::json!({
        "content": [{ "type": "text", "text": uniform_array_json(20) }],
    }));

    let assert = Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--droid"])
        .write_stdin(stdin)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "expected hook output for content array");
}

#[test]
fn non_json_tool_response_produces_no_stdout_and_exits_0() {
    let stdin = object_payload(&serde_json::json!({
        "success": true,
        "output": "just some prose, nothing structured here",
    }));

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--droid"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn malformed_stdin_json_produces_no_stdout_and_exits_0() {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--droid"])
        .write_stdin("{ this is not valid JSON at all")
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn empty_stdin_produces_no_stdout_and_exits_0() {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--droid"])
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::eq(""));
}
