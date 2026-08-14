// SPDX-License-Identifier: AGPL-3.0-only

//! Zero-allocation regression tests for the zero-copy TOON conversion APIs.
//!
//! These tests live in a dedicated integration-test binary so that `heapster`
//! only sees allocations from this test, avoiding cross-test noise.

#![forbid(unsafe_code)]

use std::alloc::System;

use tooned_convert::{maybe_tooned, maybe_tooned_in, toon_from_value};
use tooned_types::ConversionOptions;

/// Representative cross-format inputs. The conversion hot path (`toon_from_value`)
/// receives a `serde_json::Value`, so we parse each doctype outside the heapster
/// measurement and then assert that `toon_from_value` itself is allocation-free.
fn parse_representative_input(
    input: &[u8],
    doctype: tooned_types::DocType,
) -> Result<serde_json::Value, tooned_parse::ParseError> {
    match doctype {
        tooned_types::DocType::Json => tooned_json::parse_json(input),
        tooned_types::DocType::Yaml => tooned_yaml::parse_yaml(input),
        tooned_types::DocType::Toml => tooned_toml::parse_toml(input),
        tooned_types::DocType::Csv => tooned_csv::parse_csv(input),
        tooned_types::DocType::Xml => tooned_xml::parse(input),
        _ => Err(tooned_parse::ParseError::Json("unsupported representative doctype".into())),
    }
}

#[global_allocator]
static GLOBAL: heapster::Heapster<System> = heapster::Heapster::new(System);

fn zero_alloc_opts() -> ConversionOptions {
    ConversionOptions {
        dict_enabled: false,
        auto_margin: false,
        entropy_gate: false,
        cache_stable: false,
        precise_tokens: false,
        zero_alloc: true,
        ..ConversionOptions::default()
    }
}

#[test]
fn toon_from_value_is_zero_allocation_on_representative_values() {
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        return;
    }

    let cases: &[&str] = &[
        r#"{"name":"alice","age":30}"#,
        r#"[{"id":1,"name":"a"},{"id":2,"name":"b"}]"#,
        r#"{"a":1,"b":[1,2,3],"c":{"d":"e"}}"#,
    ];

    let mut out = String::with_capacity(2 * 1024 * 1024);
    let opts = zero_alloc_opts();

    for case in cases {
        let value: serde_json::Value = serde_json::from_str(case).expect("valid JSON");

        // Warm the thread-local verification scratch and `out` capacity.
        out.clear();
        let _ = toon_from_value(&value, &opts, &mut out);

        let (_, diff) = GLOBAL.measure(|| {
            out.clear();
            toon_from_value(&value, &opts, &mut out).expect("convertible value")
        });
        assert_eq!(diff.alloc_count, 0, "toon_from_value({case:?}) must not allocate");
        assert_eq!(diff.alloc_sum, 0, "toon_from_value({case:?}) must not allocate bytes");
    }
}

#[test]
fn toon_from_value_is_zero_allocation_on_cross_format_representative_inputs() {
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        return;
    }

    let cases: &[(tooned_types::DocType, &[u8])] = &[
        (
            tooned_types::DocType::Json,
            br#"[{"id":1,"name":"alice","active":true},{"id":2,"name":"bob","active":false}]"#,
        ),
        (
            tooned_types::DocType::Yaml,
            b"server:\n  host: localhost\n  port: 8080\nusers:\n  - name: alice\n    age: 30\n  - name: bob\n    age: 25\n",
        ),
        (
            tooned_types::DocType::Toml,
            b"[server]\nhost = 'localhost'\nport = 8080\n[[users]]\nname = 'alice'\nage = 30\n[[users]]\nname = 'bob'\nage = 25\n",
        ),
        (
            tooned_types::DocType::Csv,
            b"id,name,active\n1,alice,true\n2,bob,false\n",
        ),
        (
            tooned_types::DocType::Xml,
            b"<?xml version=\"1.0\"?><catalog version=\"1.0\"><book id=\"bk101\">XML Dev</book><book id=\"bk102\">Midnight Rain</book></catalog>",
        ),
    ];

    let mut out = String::with_capacity(2 * 1024 * 1024);
    let opts = zero_alloc_opts();

    for (doctype, input) in cases {
        let value = parse_representative_input(input, *doctype).unwrap();

        // Warm the thread-local verification and JSON-bytes scratch buffers
        // as well as the caller-supplied `out` capacity for this shape.
        out.clear();
        let _ = toon_from_value(&value, &opts, &mut out);

        let (_, diff) = GLOBAL.measure(|| {
            out.clear();
            toon_from_value(&value, &opts, &mut out).expect("convertible value")
        });
        assert_eq!(diff.alloc_count, 0, "toon_from_value for {doctype:?} input must not allocate");
        assert_eq!(
            diff.alloc_sum, 0,
            "toon_from_value for {doctype:?} input must not allocate bytes"
        );
    }
}

#[test]
fn maybe_tooned_in_is_zero_allocation_on_passthrough() {
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        return;
    }

    let inputs: &[&[u8]] = &[b"just some prose, nothing structured here", b""];

    let opts = zero_alloc_opts();
    let mut out = String::with_capacity(2 * 1024 * 1024);

    for input in inputs {
        let (_, diff) = GLOBAL.measure(|| {
            out.clear();
            maybe_tooned_in(input, &opts, &mut out).expect("infallible")
        });
        assert_eq!(diff.alloc_count, 0, "maybe_tooned_in passthrough must not allocate");
        assert_eq!(diff.alloc_sum, 0, "maybe_tooned_in passthrough must not allocate bytes");
    }
}

#[test]
fn maybe_tooned_is_zero_allocation_on_passthrough() {
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        return;
    }

    let inputs: &[&[u8]] = &[b"just some prose, nothing structured here", b""];
    let opts = zero_alloc_opts();

    for input in inputs {
        let (_, diff) = GLOBAL.measure(|| maybe_tooned(input, &opts).expect("infallible"));
        assert_eq!(diff.alloc_count, 0, "maybe_tooned passthrough must not allocate");
        assert_eq!(diff.alloc_sum, 0, "maybe_tooned passthrough must not allocate bytes");
    }
}
