// SPDX-License-Identifier: AGPL-3.0-only

//! T009: no-panic property + example tests (constitution Principle I;
//! contract postcondition in `contracts/tooned-core-api.md`).
//!
//! `maybe_tooned` and `inspect` MUST NOT panic for any `&[u8]` input,
//! including invalid UTF-8, truncated multi-byte sequences, and
//! adversarially deep nesting. A panic anywhere in this file fails the test
//! by construction -- there is no explicit "did not panic" assertion to
//! write; proptest/the test harness itself catches it.

mod common;

use proptest::prelude::*;
use tooned_core::{ConversionOptions, DocType, inspect, maybe_tooned};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let opts = ConversionOptions::default();
        let _ = maybe_tooned(&bytes, &opts);
        let _ = inspect(&bytes, &opts);
    }

    #[test]
    fn never_panics_on_structured_json(value in common::arb_json_value()) {
        let bytes = common::to_json_bytes(&value);
        let opts = ConversionOptions::default();
        let _ = maybe_tooned(&bytes, &opts);
        let _ = inspect(&bytes, &opts);
    }

    #[test]
    fn never_panics_with_adversarial_options(
        bytes in proptest::collection::vec(any::<u8>(), 0..512),
        margin_pct in prop_oneof![
            Just(f64::NAN),
            Just(f64::INFINITY),
            Just(f64::NEG_INFINITY),
            -1000.0..1000.0,
        ],
        max_input_bytes in 0usize..64,
        precise_tokens in any::<bool>(),
    ) {
        let opts = ConversionOptions {
            margin_pct,
            max_input_bytes,
            format_hint: None,
            precise_tokens,
            auto_margin: false,
            dict_enabled: true,
            critical_policy: tooned_types::CriticalFieldPolicy::default_policy(),
            entropy_gate: true,
            tokenizer: None,
        };
        let _ = maybe_tooned(&bytes, &opts);
        let _ = inspect(&bytes, &opts);
    }
}

#[test]
fn never_panics_on_invalid_utf8() {
    let bytes: &[u8] = &[0xFF, 0xFE, 0x00, 0x7B, 0x22, 0xC0, 0x80, 0xED, 0xA0, 0x80];
    let opts = ConversionOptions::default();
    let _ = maybe_tooned(bytes, &opts);
    let _ = inspect(bytes, &opts);
}

#[test]
fn never_panics_on_truncated_multibyte_sequence() {
    // A JSON string opener followed by a truncated 4-byte UTF-8 sequence.
    let bytes: &[u8] = b"{\"x\": \"\xF0\x9F\x98";
    let opts = ConversionOptions::default();
    let _ = maybe_tooned(bytes, &opts);
    let _ = inspect(bytes, &opts);
}

#[test]
fn never_panics_on_truncated_multibyte_sequence_yaml() {
    let bytes: &[u8] = b"key: \xE2\x28\xA1 value\n";
    let opts =
        ConversionOptions { format_hint: Some(DocType::Yaml), ..ConversionOptions::default() };
    let _ = maybe_tooned(bytes, &opts);
    let _ = inspect(bytes, &opts);
}

#[test]
fn never_panics_on_adversarially_deep_nesting() {
    let depth = 100_000;
    let mut bytes = Vec::with_capacity(depth * 2);
    bytes.extend(std::iter::repeat_n(b'[', depth));
    bytes.extend(std::iter::repeat_n(b']', depth));
    let opts =
        ConversionOptions { max_input_bytes: 8 * 1024 * 1024, ..ConversionOptions::default() };
    let _ = maybe_tooned(&bytes, &opts);
    let _ = inspect(&bytes, &opts);
}

#[test]
fn never_panics_on_explicit_format_hint_mismatch() {
    let bytes: &[u8] = b"not even close to any of these formats {}[]===";
    let opts =
        ConversionOptions { format_hint: Some(DocType::Toml), ..ConversionOptions::default() };
    let _ = maybe_tooned(bytes, &opts);
    let _ = inspect(bytes, &opts);
}

#[test]
fn never_panics_on_empty_input() {
    let opts = ConversionOptions::default();
    let _ = maybe_tooned(b"", &opts);
    let _ = inspect(b"", &opts);
}
