// SPDX-License-Identifier: AGPL-3.0-only

//! Shared proptest generators for tooned-core's Foundational-phase safety-
//! invariant property tests (T007-T009).
//!
//! `arb_uniform_array` is deliberately biased toward uniform arrays of
//! objects sharing a key set (repeated keys) since that's the payload shape
//! TOON's tabular encoding actually shrinks -- without this bias, a fully
//! generic JSON generator would rarely produce anything that beats compact
//! JSON, and the round-trip / never-regression properties would only ever
//! be exercised via their vacuously-true Passthrough branch.
//!
//! `mod common;` is duplicated per integration-test binary (a `tests/`
//! convention), so any one binary that doesn't use every helper here would
//! otherwise warn on dead code -- allowed at module level rather than
//! per-item since which helpers go unused varies by which test file is
//! compiling this module.
#![allow(dead_code)]

pub mod xml;

use proptest::prelude::*;
use serde_json::{Map, Value};

/// A JSON scalar: null, bool, small integer, or a short ASCII string.
/// Deliberately excludes floats: TOON's decoder has a documented edge case
/// where whole-number floats (e.g. `1.0`) round-trip back as integers
/// (`Number(1)` != `Number(1.0)`), which is intentionally *not* what these
/// generators exist to explore (that's covered directly, deterministically,
/// by `convert.rs`'s `round_trip_mismatch_downgrades_to_passthrough` test) --
/// including it here would make the round-trip property fail for reasons
/// unrelated to what these generic fuzz tests are checking.
pub fn arb_scalar() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        (-1_000_000i64..1_000_000).prop_map(Value::from),
        "[a-zA-Z0-9_ ]{0,12}".prop_map(Value::String),
    ]
}

/// One JSON object with a fixed key set (one scalar per key) -- the
/// per-row shape a uniform array of objects needs for TOON's tabular
/// encoding to actually pay off.
fn arb_row(keys: Vec<String>) -> impl Strategy<Value = Value> {
    proptest::collection::vec(arb_scalar(), keys.len()).prop_map(move |vals| {
        let mut map = Map::new();
        for (k, v) in keys.iter().zip(vals) {
            map.insert(k.clone(), v);
        }
        Value::Object(map)
    })
}

/// A top-level array of 2..12 objects all sharing the same small key set --
/// the payload shape this feature exists to shrink.
pub fn arb_uniform_array() -> impl Strategy<Value = Value> {
    proptest::collection::vec("[a-z]{1,8}", 1..5usize).prop_flat_map(|keys| {
        let row = arb_row(keys);
        proptest::collection::vec(row, 2..12).prop_map(Value::Array)
    })
}

/// Any JSON value: scalars, plus bounded-depth arrays/objects. Used by the
/// no-panic and general round-trip/never-regression property tests, which
/// care about arbitrary shapes, not specifically convertible ones.
pub fn arb_json_value() -> impl Strategy<Value = Value> {
    let leaf = arb_scalar();
    leaf.prop_recursive(3, 32, 6, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            proptest::collection::vec(("[a-z]{1,6}", inner), 0..6).prop_map(|pairs| {
                let mut map = Map::new();
                for (k, v) in pairs {
                    map.insert(k, v);
                }
                Value::Object(map)
            }),
        ]
    })
}

/// Serializes `value` to compact JSON bytes for use as `maybe_tooned` input.
pub fn to_json_bytes(value: &Value) -> Vec<u8> {
    match serde_json::to_vec(value) {
        Ok(bytes) => bytes,
        Err(_) => b"null".to_vec(),
    }
}
