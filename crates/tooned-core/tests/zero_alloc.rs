// SPDX-License-Identifier: AGPL-3.0-only

//! Zero-allocation regression tests for the re-exported conversion APIs.

#![forbid(unsafe_code)]

use std::alloc::System;

use tooned_core::{Conversion, ConversionOptions, maybe_tooned, maybe_tooned_in, toon_from_value};

/// Parse a small CSV/TSV table into the same `Value` shape `tooned-csv` uses:
/// an array of objects with all-string field values.
fn parse_csv_like(input: &[u8], delimiter: u8) -> Result<serde_json::Value, csv::Error> {
    let mut reader =
        csv::ReaderBuilder::new().delimiter(delimiter).has_headers(true).from_reader(input);
    let headers = reader.headers()?.iter().map(ToString::to_string).collect::<Vec<_>>();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        let mut map = serde_json::Map::with_capacity(headers.len());
        for (i, field) in record.iter().enumerate() {
            let key = headers.get(i).cloned().unwrap_or_else(|| format!("field_{i}"));
            map.insert(key, serde_json::Value::String(field.to_string()));
        }
        rows.push(serde_json::Value::Object(map));
    }
    Ok(serde_json::Value::Array(rows))
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
fn toon_from_value_is_zero_allocation() {
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        return;
    }

    let value: serde_json::Value =
        serde_json::from_str(r#"[{"id":1,"name":"a"},{"id":2,"name":"b"}]"#).unwrap();
    let opts = zero_alloc_opts();
    let mut out = String::with_capacity(2 * 1024 * 1024);

    // Warm the verification scratch.
    out.clear();
    let _ = toon_from_value(&value, &opts, &mut out);

    let (_, diff) = GLOBAL.measure(|| {
        out.clear();
        toon_from_value(&value, &opts, &mut out).expect("convertible")
    });
    assert_eq!(diff.alloc_count, 0);
    assert_eq!(diff.alloc_sum, 0);
}

#[test]
fn toon_from_value_is_zero_allocation_on_cross_format_representative_inputs() {
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        return;
    }

    let cases: &[(&str, serde_json::Value)] = &[
        (
            "json",
            serde_json::from_str(r#"[{"id":1,"name":"alice","active":true},{"id":2,"name":"bob","active":false}]"#).unwrap(),
        ),
        (
            "yaml",
            serde_yaml::from_slice::<serde_json::Value>(b"server:\n  host: localhost\n  port: 8080\nusers:\n  - name: alice\n    age: 30\n  - name: bob\n    age: 25\n").unwrap(),
        ),
        (
            "toml",
            toml::from_str::<serde_json::Value>(
                "[server]\nhost = 'localhost'\nport = 8080\n[[users]]\nname = 'alice'\nage = 30\n[[users]]\nname = 'bob'\nage = 25\n"
            ).unwrap(),
        ),
        (
            "csv",
            parse_csv_like(b"id,name,active\n1,alice,true\n2,bob,false\n", b',').unwrap(),
        ),
        (
            "xml",
            tooned_core::xml::parse(b"<?xml version=\"1.0\"?><catalog version=\"1.0\"><book id=\"bk101\">XML Dev</book><book id=\"bk102\">Midnight Rain</book></catalog>").unwrap(),
        ),
    ];

    let mut out = String::with_capacity(2 * 1024 * 1024);
    let opts = zero_alloc_opts();

    for (label, value) in cases {
        // Warm the thread-local verification and JSON-bytes scratch buffers
        // as well as the caller-supplied `out` capacity for this shape.
        out.clear();
        let _ = toon_from_value(value, &opts, &mut out);

        let (_, diff) = GLOBAL.measure(|| {
            out.clear();
            toon_from_value(value, &opts, &mut out).expect("convertible value")
        });
        assert_eq!(diff.alloc_count, 0, "toon_from_value for {label} input must not allocate");
        assert_eq!(diff.alloc_sum, 0, "toon_from_value for {label} input must not allocate bytes");
    }
}

#[test]
fn maybe_tooned_in_passthrough_is_zero_allocation() {
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        return;
    }

    let input = b"plain text, not structured";
    let opts = zero_alloc_opts();
    let mut out = String::with_capacity(2 * 1024 * 1024);

    let (result, diff) = GLOBAL.measure(|| {
        out.clear();
        maybe_tooned_in(input, &opts, &mut out).expect("infallible")
    });
    assert!(matches!(result, Conversion::Passthrough { .. }));
    assert_eq!(diff.alloc_count, 0);
    assert_eq!(diff.alloc_sum, 0);
}

#[test]
fn maybe_tooned_passthrough_is_zero_allocation() {
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        return;
    }

    let input = b"plain text, not structured";
    let opts = zero_alloc_opts();

    let (result, diff) = GLOBAL.measure(|| maybe_tooned(input, &opts).expect("infallible"));
    assert!(matches!(result, Conversion::Passthrough { .. }));
    assert_eq!(diff.alloc_count, 0);
    assert_eq!(diff.alloc_sum, 0);
}
