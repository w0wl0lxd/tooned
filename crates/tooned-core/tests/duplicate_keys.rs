//! T013: a JSON object with duplicate keys must produce identical
//! `maybe_tooned` output via the `sonic-rs` fast path and the `serde_json`
//! fallback path (research.md #4's duplicate-key caveat).
//!
//! `sonic-rs`'s own docs warn that its native `sonic_rs::Value` differs from
//! `serde_json::Value` on duplicate-key handling; `tooned-core` avoids that
//! trap entirely by always deserializing straight into `serde_json::Value`
//! (`sonic_rs::from_slice::<serde_json::Value>`), never through
//! `sonic_rs::Value`. This test proves that choice is actually equivalent to
//! plain `serde_json`, both directly (parser-to-parser) and end-to-end
//! (through `maybe_tooned`, at sizes that straddle
//! `tooned_core::SONIC_RS_THRESHOLD_BYTES`).

use serde_json::Value;
use tooned_core::{
    Conversion, ConversionOptions, SONIC_RS_THRESHOLD_BYTES, decode_toon, maybe_tooned,
};

/// A single object with duplicate `id`/`name` keys -- last value wins, per
/// both `serde_json` and (per research.md #4, once verified) `sonic-rs`.
fn duplicate_key_object(i: usize) -> String {
    format!(r#"{{"id":{i},"id":{i},"name":"row-{i}","name":"row-{i}-final","active":true}}"#)
}

/// Wraps `count` duplicate-key objects sharing the same schema in a JSON
/// array. Large `count` pushes the payload's byte size past
/// `SONIC_RS_THRESHOLD_BYTES`, exercising the sonic-rs fast path; small
/// `count` stays well under it, exercising plain `serde_json`.
fn build_array(count: usize) -> Vec<u8> {
    let mut s = String::from("[");
    for i in 0..count {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&duplicate_key_object(i));
    }
    s.push(']');
    s.into_bytes()
}

#[test]
fn sonic_rs_and_serde_json_resolve_duplicate_keys_identically_small() {
    let bytes = build_array(1);
    assert!(bytes.len() < SONIC_RS_THRESHOLD_BYTES);
    let via_sonic: Value = sonic_rs::from_slice(&bytes).expect("valid JSON");
    let via_serde: Value = serde_json::from_slice(&bytes).expect("valid JSON");
    assert_eq!(via_sonic, via_serde);
}

#[test]
fn sonic_rs_and_serde_json_resolve_duplicate_keys_identically_large() {
    let mut count = 200;
    let mut bytes = build_array(count);
    while bytes.len() < SONIC_RS_THRESHOLD_BYTES {
        count *= 2;
        bytes = build_array(count);
    }
    let via_sonic: Value = sonic_rs::from_slice(&bytes).expect("valid JSON");
    let via_serde: Value = serde_json::from_slice(&bytes).expect("valid JSON");
    assert_eq!(via_sonic, via_serde);
}

#[test]
fn maybe_tooned_resolves_duplicate_keys_identically_below_and_above_threshold() {
    let small = build_array(1);
    assert!(small.len() < SONIC_RS_THRESHOLD_BYTES);

    let mut count = 200;
    let mut large = build_array(count);
    while large.len() < SONIC_RS_THRESHOLD_BYTES {
        count *= 2;
        large = build_array(count);
    }

    let opts = ConversionOptions { margin_pct: 0.0, ..ConversionOptions::default() };

    for bytes in [small, large] {
        let expected: Value = serde_json::from_slice(&bytes).expect("valid JSON");
        let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error");
        if let Conversion::Toon { text, .. } = result {
            let decoded = decode_toon(&text).expect("a Conversion::Toon's own text must decode");
            assert_eq!(
                decoded, expected,
                "duplicate-key resolution must match serde_json's last-value-wins semantics"
            );
        }
        // Passthrough is also an acceptable outcome here (this test's point
        // is *consistency*, not forcing a specific decision); the direct
        // parser-equivalence tests above cover the core claim unconditionally.
    }
}
