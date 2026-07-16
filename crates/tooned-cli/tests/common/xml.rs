//! Shared XML fixture helpers for `tooned-cli` integration tests.
#![allow(dead_code)]

use std::fmt::Write as _;

/// A record-list XML payload with `rows` repeated `<record>` elements, each
/// carrying a fixed set of attributes. The shape is chosen to reliably win
/// the TOON-vs-JSON comparison.
pub fn xml_record_list(rows: usize) -> String {
    let mut s = String::from("<?xml version=\"1.0\"?>\n<data>\n");
    for i in 0..rows {
        let _ = writeln!(s, "<record id=\"{i}\" name=\"row-{i}\" active=\"true\" score=\"{i}\" />");
    }
    s.push_str("</data>");
    s
}

/// A tiny XML payload with a long text element, deliberately large enough
/// that TOON is not smaller by the default 2% margin, so the adaptive pipe
/// should pass the original XML through unchanged.
pub fn long_text_xml(text_len: usize) -> String {
    let mut s = String::from("<?xml version=\"1.0\"?><root>");
    let _ = write!(s, "{}", "x".repeat(text_len));
    s.push_str("</root>");
    s
}
