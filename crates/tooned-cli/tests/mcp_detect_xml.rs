// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


//! Contract test for `tooned_detect` with XML format hint.
//! See `specs/002-xml-conversion/contracts/mcp-tools.md`.

mod common;

use common::mcp_client::McpClient;
use serde_json::json;

#[allow(clippy::expect_used)]
fn field<'a>(value: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    value.get(key).expect("expected field")
}

#[test]
fn tooned_detect_reports_xml_and_would_convert_for_valid_xml() {
    let mut client = McpClient::spawn();
    let content = common::xml::xml_record_list(10);

    let response =
        client.call_tool("tooned_detect", &json!({ "content": content, "format_hint": "xml" }));
    let result = field(&response, "result");
    let structured = field(result, "structuredContent");
    assert_eq!(field(structured, "doc_type"), &json!("xml"));
    let shape = field(structured, "shape");
    assert_eq!(field(shape, "kind"), &json!("scalar"));
    assert_eq!(field(structured, "would_convert"), &json!(true));
    // tooned_detect never performs the conversion -- no TOON text anywhere.
    assert!(structured.get("text").is_none());
}

#[test]
fn tooned_detect_reports_xml_parse_failure_passthrough_for_malformed_xml() {
    let mut client = McpClient::spawn();
    let content = "just some prose, nothing structured here";

    let response =
        client.call_tool("tooned_detect", &json!({ "content": content, "format_hint": "xml" }));
    let result = field(&response, "result");
    let structured = field(result, "structuredContent");
    assert_eq!(field(structured, "doc_type"), &json!("xml"));
    assert_eq!(field(structured, "would_convert"), &json!(false));
    let reason = field(structured, "reason");
    assert_eq!(field(reason, "kind"), &json!("parse_failed"));
    assert!(structured.get("text").is_none());
}
