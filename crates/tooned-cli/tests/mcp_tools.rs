// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Contract tests for `tooned_convert`/`tooned_detect`/`tooned_decode`
//! (T073). See `specs/001-adaptive-toon-conversion/contracts/mcp-tools.md`.
//!
//! Drives the real `tooned mcp serve` binary over its actual stdio JSON-RPC
//! transport (`tests/common/mcp_client.rs`) rather than calling internal
//! Rust functions directly -- this is a contract test of the wire protocol
//! a real MCP client would speak, not a unit test of the handler.

mod common;

use std::fmt::Write as _;

use common::mcp_client::McpClient;
use serde_json::{Value, json};

fn uniform_array_json(rows: usize) -> String {
    let mut s = String::from("[");
    for i in 0..rows {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, r#"{{"id":{i},"name":"row-{i}","active":true,"score":{i}}}"#);
    }
    s.push(']');
    s
}

/// `Value` indexing (`v["key"]`) is off-limits under `clippy::indexing_slicing`
/// (denied workspace-wide, including tests); every field access below goes
/// through explicit `.get(...).expect(...)` instead.
#[allow(clippy::expect_used)] // test-only helper in an integration-test binary, not `cfg(test)`-scoped
fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).expect("expected field")
}

fn is_tool_error(result: &Value) -> bool {
    result.get("isError").and_then(Value::as_bool) == Some(true)
}

#[test]
fn tooned_convert_converts_uniform_array_and_reports_savings() {
    let mut client = McpClient::spawn();
    let content = uniform_array_json(30);

    let response = client.call_tool("tooned_convert", &json!({ "content": content }));
    let result = field(&response, "result");
    assert!(!is_tool_error(result), "expected a successful conversion, got: {result}");
    let structured = field(result, "structuredContent");
    assert_eq!(field(structured, "converted"), &json!(true));
    let text = field(structured, "text").as_str().expect("text field");
    assert!(text.len() < content.len());
    let report = field(structured, "report");
    assert_eq!(field(report, "doc_type"), &json!("json"));
    let toon_bytes = field(report, "toon_bytes").as_u64().expect("toon_bytes");
    let json_bytes = field(report, "json_bytes").as_u64().expect("json_bytes");
    assert!(toon_bytes < json_bytes);
}

#[test]
fn tooned_convert_passes_through_non_structured_content_unchanged() {
    let mut client = McpClient::spawn();
    let content = "just some prose, nothing structured here".to_string();

    let response = client.call_tool("tooned_convert", &json!({ "content": content }));
    let result = field(&response, "result");
    let structured = field(result, "structuredContent");
    assert_eq!(field(structured, "converted"), &json!(false));
    assert_eq!(field(structured, "text"), &json!(content));
    assert_eq!(structured.get("report"), Some(&Value::Null));
    // Regression: a passthrough result must surface *why* it declined to
    // convert (finding: this used to be silently discarded, forcing a
    // second `tooned_detect` call to find out).
    let reason = field(structured, "reason");
    assert_eq!(field(reason, "kind"), &json!("not_structured_data"));
}

#[test]
fn tooned_detect_reports_shape_without_performing_conversion() {
    let mut client = McpClient::spawn();
    let content = uniform_array_json(10);

    let response = client.call_tool("tooned_detect", &json!({ "content": content }));
    let result = field(&response, "result");
    let structured = field(result, "structuredContent");
    assert_eq!(field(structured, "doc_type"), &json!("json"));
    let shape = field(structured, "shape");
    assert_eq!(field(shape, "kind"), &json!("uniform_array_of_objects"));
    assert_eq!(field(structured, "would_convert"), &json!(true));
    // tooned_detect never performs the conversion -- no TOON text anywhere
    // in the structured output (contract: "no conversion performed").
    assert!(structured.get("text").is_none());
}

#[test]
fn tooned_decode_round_trips_a_converted_document() {
    let mut client = McpClient::spawn();
    let content = uniform_array_json(30);

    let convert_response = client.call_tool("tooned_convert", &json!({ "content": content }));
    let convert_result = field(&convert_response, "result");
    let convert_structured = field(convert_result, "structuredContent");
    let converted_text =
        field(convert_structured, "text").as_str().expect("converted text").to_string();

    let decode_response = client.call_tool("tooned_decode", &json!({ "toon": converted_text }));
    let result = field(&decode_response, "result");
    assert!(!is_tool_error(result));
    let structured = field(result, "structuredContent");
    let decoded_value = field(structured, "value");
    let original_value: Value =
        serde_json::from_str(&content).expect("original content is valid JSON");
    assert_eq!(*decoded_value, original_value);
}

#[test]
fn tooned_decode_reports_a_tool_level_error_on_invalid_toon_not_a_crash() {
    let mut client = McpClient::spawn();

    let response = client.call_tool("tooned_decode", &json!({ "toon": "\"unterminated" }));
    let result = response.get("result").expect(
        "an invalid-TOON decode must still be a tools/call `result` (tool-level error), \
         never a JSON-RPC protocol-level `error` -- the server itself must not fault",
    );
    assert!(is_tool_error(result));
}
