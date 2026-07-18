// SPDX-License-Identifier: AGPL-3.0-only

//! Zero-allocation tests for toon_from_value and maybe_tooned passthrough.
//!
//! These tests assert that the in-place conversion hot path performs no heap
//! allocations once the thread-local scratch buffer has been warmed. There is a
//! single `#[test]` so measurements are not interleaved with other tests in the
//! same binary.

#![forbid(unsafe_code)]

use std::alloc::System;

use serde_json::json;
use tooned_convert::toon_from_value;
use tooned_types::{Conversion, ConversionOptions};

#[global_allocator]
static GLOBAL: heapster::Heapster<System> = heapster::Heapster::new(System);

fn zero_alloc_opts() -> ConversionOptions {
    ConversionOptions {
        dict_enabled: false,
        auto_margin: false,
        entropy_gate: false,
        precise_tokens: false,
        ..ConversionOptions::default()
    }
}

#[test]
fn toon_conversion_hot_path_is_zero_alloc() {
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        return;
    }

    // Simple object
    let value = json!({"name": "Alice", "age": 30, "active": true});
    let opts = zero_alloc_opts();
    let mut warm_out = String::with_capacity(1024);
    for _ in 0..10 {
        let _ = toon_from_value(&value, &opts, &mut warm_out);
    }
    let mut out = String::with_capacity(1024);
    let (_, diff) = GLOBAL.measure(|| toon_from_value(&value, &opts, &mut out));
    assert_eq!(diff.alloc_count, 0, "toon_from_value allocated {} times", diff.alloc_count);
    assert_eq!(diff.alloc_sum, 0, "toon_from_value allocated {} bytes", diff.alloc_sum);

    // Array of objects
    let value = json!([
        {"id": 1, "name": "Alice"},
        {"id": 2, "name": "Bob"},
        {"id": 3, "name": "Charlie"},
    ]);
    let opts = zero_alloc_opts();
    let mut warm_out = String::with_capacity(1024);
    for _ in 0..10 {
        let _ = toon_from_value(&value, &opts, &mut warm_out);
    }
    let mut out = String::with_capacity(1024);
    let (_, diff) = GLOBAL.measure(|| toon_from_value(&value, &opts, &mut out));
    assert_eq!(diff.alloc_count, 0, "toon_from_value allocated {} times", diff.alloc_count);
    assert_eq!(diff.alloc_sum, 0, "toon_from_value allocated {} bytes", diff.alloc_sum);

    // maybe_tooned_in passthrough
    let input: &[u8] = b"just some prose, nothing structured here";
    let opts = ConversionOptions::default();
    let mut warm_out = String::with_capacity(1024);
    for _ in 0..10 {
        let _ = tooned_convert::maybe_tooned_in(input, &opts, &mut warm_out);
    }
    let mut out = String::with_capacity(1024);
    let (conversion, diff) =
        GLOBAL.measure(|| tooned_convert::maybe_tooned_in(input, &opts, &mut out));
    assert!(matches!(conversion.unwrap(), Conversion::Passthrough { .. }));
    assert_eq!(
        diff.alloc_count, 0,
        "maybe_tooned_in passthrough allocated {} times",
        diff.alloc_count
    );
    assert_eq!(diff.alloc_sum, 0, "maybe_tooned_in passthrough allocated {} bytes", diff.alloc_sum);
}
