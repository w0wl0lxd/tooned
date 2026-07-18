// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Integration tests for `tooned hook run --droid`.
//!
//! Droid `PostToolUse` stdin carries `tool_response` as either a raw string
//! or an object whose schema is tool-specific; `hooks/mod.rs` extracts
//! common string fields (`output`, `content`, `stdout`, `result`, `text`)
//! and MCP-style `content` arrays. Droid only supports `additionalContext` for
//! PostToolUse, which would append the TOON to the original JSON rather than
//! replace it, so `tooned` does not emit anything for Droid. Use
//! `tooned wrap -- <cmd>` or `... | tooned pipe` when TOON-only output is
//! required. See <https://docs.factory.ai/reference/hooks-reference>.

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
fn convertible_output_field_passthroughs_and_exits_0() {
    let tool_response = uniform_array_json(20);
    let stdin = object_payload(&serde_json::json!({
        "success": true,
        "output": tool_response,
        "error": null,
    }));

    // Droid cannot replace the native tool output, and `additionalContext`
    // would inflate total context, so the hook passthroughs.
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--droid"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
}

#[test]
fn string_tool_response_also_passthroughs() {
    let stdin = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Execute",
        "tool_input": { "command": "cat data.json" },
        "tool_response": uniform_array_json(20),
    })
    .to_string();

    // Extraction works, but Droid cannot replace the output, so no stdout.
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--droid"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
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
fn content_array_with_only_json_text_item_passthroughs() {
    let stdin = object_payload(&serde_json::json!({
        "content": [{ "type": "text", "text": uniform_array_json(20) }],
    }));

    // Droid cannot replace the output, so extraction alone does not emit stdout.
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--droid"])
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::eq(""));
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
