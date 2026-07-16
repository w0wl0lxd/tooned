// SPDX-License-Identifier: AGPL-3.0-only

//! XML-specific proptest generators for `tooned-core` XML property tests.
//!
//! `arb_xml_record_list` builds a record-list XML document whose `tooned-core`
//! parser is expected to produce a uniform `Value` (an object wrapping a
//! uniform array of objects keyed by `@`-prefixed attribute names) -- the
//! shape TOON's tabular encoding shrinks well. Attribute values are restricted
//! to safe characters (letters, digits, underscore, space) so the generated
//! XML never requires escaping beyond `quick-xml`'s own writer.
#![allow(dead_code)]

use std::fmt::Write as _;

use proptest::prelude::*;
use serde_json::{Map, Value};

/// A safe XML element/attribute name: starts with a letter, followed by up to
/// seven letters/digits/underscores.
fn arb_xml_name() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9_]{0,7}"
}

/// A safe attribute value: letters, digits, underscore, or space. Deliberately
/// excludes `"`, `'`, `&`, `<`, and `>` so raw XML string assembly is safe.
fn arb_attr_value() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_ ]{0,12}"
}

/// A root element name for the generated XML record list. Restricted to a
/// small set of non-HTML tags so the sniffer doesn't reject the payload.
fn arb_root_name() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("data"), Just("records"), Just("root"), Just("items"), Just("catalog"),]
}

/// A child element name for the repeated record items. Restricted to a small
/// set of non-HTML tags.
fn arb_child_name() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("record"), Just("item"), Just("entry"), Just("row"),]
}

/// Generates `(xml_bytes, expected_value)` pairs: a record-list XML document
/// and the `Value` `tooned_core::xml::parse` should produce for it.
///
/// The output is a top-level object with a single key (the root tag name)
/// mapping to an object whose single key (the child tag name) maps to an array
/// of objects. Each child object has `@`-prefixed attribute keys from the
/// generated attribute set, all sharing the same keys across rows.
pub fn arb_xml_record_list() -> impl Strategy<Value = (Vec<u8>, Value)> {
    (arb_root_name(), arb_child_name(), proptest::collection::vec(arb_xml_name(), 1..8usize))
        .prop_flat_map(|(root, child, keys)| {
            let row = proptest::collection::vec(arb_attr_value(), keys.len());
            proptest::collection::vec(row, 2..20usize).prop_map(move |rows| {
                // Stable, deduplicated attribute key order so the expected
                // value and the generated XML attribute order match exactly.
                let mut keys = keys.clone();
                keys.sort();
                keys.dedup();
                keys.retain(|k| k != "xml" && k != "xmlns");

                let mut xml = String::new();
                let _ = write!(xml, "<{root}>");

                let mut rows_value: Vec<Value> = Vec::with_capacity(rows.len());
                for vals in rows {
                    let mut tag = format!("<{child}");
                    let mut row_map = Map::new();
                    for (k, v) in keys.iter().zip(vals) {
                        let _ = write!(tag, " {k}=\"{v}\"");
                        row_map.insert(format!("@{k}"), Value::String(v));
                    }
                    tag.push_str(" />");
                    xml.push_str(&tag);
                    rows_value.push(Value::Object(row_map));
                }

                let _ = write!(xml, "</{root}>");

                let mut child_map = Map::new();
                child_map.insert(child.to_string(), Value::Array(rows_value));

                let mut root_map = Map::new();
                root_map.insert(root.to_string(), Value::Object(child_map));

                (xml.into_bytes(), Value::Object(root_map))
            })
        })
}
