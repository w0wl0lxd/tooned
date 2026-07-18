// SPDX-License-Identifier: AGPL-3.0-only

//! Round-trip regression tests that cross-check `toon_lsp::toon::verify_round_trip`
//! against `tooned_toon::decode_toon`. Both directions must agree that a value
//! encodes to a canonical TOON text and that the decoded text equals the
//! original value.

#![forbid(unsafe_code)]

use serde_json::json;
use toon_lsp::toon::{ToonConfig, encode_into, verify_round_trip};
use tooned_toon::decode_toon;

fn roundtrip_cases() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        ("simple object", json!({"name": "Alice", "age": 30, "active": true})),
        (
            "array of flat objects",
            json!([
                {"id": 1, "name": "Alice"},
                {"id": 2, "name": "Bob"},
                {"id": 3, "name": "Charlie"},
            ]),
        ),
        ("nested object", json!({"a": {"b": {"c": 1, "d": [2, 3, 4]}}})),
        ("escaped string", json!({"msg": "hello: world, \"quoted\", tab\there"})),
        ("numeric scalar", json!({"pi": 3.5, "count": 42, "neg": -7})),
        ("hex string regression", json!({"addr": "0x0", "mask": "0xDEADBEEF"})),
        ("mixed array", json!([null, true, 1.5, "x", {"y": 2}])),
    ]
}

#[test]
fn encode_then_verify_round_trip_succeeds() {
    let config = ToonConfig::default();
    for (label, value) in roundtrip_cases() {
        let mut toon = String::new();
        encode_into(&value, &config, &mut toon).expect("encode should succeed");

        let verified =
            verify_round_trip(&toon, &value, &config).expect("verify_round_trip must not error");
        assert!(verified, "verify_round_trip failed for case: {label}\n{toon}");
    }
}

#[test]
fn encode_then_decode_toon_yields_original_value() {
    let config = ToonConfig::default();
    for (label, value) in roundtrip_cases() {
        let mut toon = String::new();
        encode_into(&value, &config, &mut toon).expect("encode should succeed");

        let decoded = decode_toon(&toon).expect("decode should succeed");
        assert_eq!(decoded, value, "decode_toon did not round-trip for case: {label}\n{toon}");
    }
}

#[test]
fn verify_round_trip_rejects_non_canonical_text() {
    let value = json!({"name": "Alice", "age": 30});
    let mut canonical = String::new();
    encode_into(&value, &ToonConfig::default(), &mut canonical).unwrap();

    // Extra whitespace makes the text non-canonical, so the verifier should
    // report a mismatch without panicking.
    let non_canonical = canonical.replace('\n', " \n");
    assert!(
        verify_round_trip(&non_canonical, &value, &ToonConfig::default()).is_err(),
        "verify_round_trip should reject non-canonical spacing"
    );
}
