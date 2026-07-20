// SPDX-License-Identifier: AGPL-3.0-only

//! Zero-allocation regression tests for the re-exported conversion APIs.

#![forbid(unsafe_code)]

use std::alloc::System;

use tooned_core::{Conversion, ConversionOptions, maybe_tooned, maybe_tooned_in, toon_from_value};

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
