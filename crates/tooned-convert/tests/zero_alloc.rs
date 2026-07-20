// SPDX-License-Identifier: AGPL-3.0-only

//! Zero-allocation regression tests for the zero-copy TOON conversion APIs.
//!
//! These tests live in a dedicated integration-test binary so that `heapster`
//! only sees allocations from this test, avoiding cross-test noise.

#![forbid(unsafe_code)]

use std::alloc::System;

use tooned_convert::{maybe_tooned, maybe_tooned_in, toon_from_value};
use tooned_types::ConversionOptions;

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
