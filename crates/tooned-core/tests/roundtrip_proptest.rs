// SPDX-License-Identifier: AGPL-3.0-only

//! T007: round-trip fidelity property test (constitution Principle IV;
//! contract postcondition in `contracts/tooned-core-api.md`).
//!
//! For every input where `maybe_tooned` returns `Conversion::Toon`,
//! `decode_toon(&text)` MUST succeed and be structurally equal to the value
//! that was encoded.

mod common;

use proptest::prelude::*;
use tooned_core::{Conversion, ConversionOptions, decode_toon, maybe_tooned};

proptest! {
    #[test]
    fn toon_output_always_round_trips_uniform_arrays(value in common::arb_uniform_array()) {
        let bytes = common::to_json_bytes(&value);
        let opts = ConversionOptions::default();
        let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error for payload-driven input");

        if let Conversion::Toon { text, .. } = result {
            let decoded = decode_toon(&text).expect("a Conversion::Toon's own text must always decode");
            let original: serde_json::Value =
                serde_json::from_slice(&bytes).expect("fixture bytes are always valid JSON");
            prop_assert_eq!(decoded, original);
        }
    }

    #[test]
    fn toon_output_always_round_trips_general_json(value in common::arb_json_value()) {
        let bytes = common::to_json_bytes(&value);
        let opts = ConversionOptions::default();
        let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error for payload-driven input");

        if let Conversion::Toon { text, .. } = result {
            let decoded = decode_toon(&text).expect("a Conversion::Toon's own text must always decode");
            let original: serde_json::Value =
                serde_json::from_slice(&bytes).expect("fixture bytes are always valid JSON");
            prop_assert_eq!(decoded, original);
        }
    }
}
