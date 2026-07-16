// SPDX-License-Identifier: AGPL-3.0-only

//! T008: never-a-regression property test (constitution Principle IV;
//! contract postcondition in `contracts/tooned-core-api.md`).
//!
//! For every input where `maybe_tooned` returns `Conversion::Toon`,
//! `report.toon_bytes < report.json_bytes` MUST hold.

mod common;

use proptest::prelude::*;
use tooned_core::{Conversion, ConversionOptions, maybe_tooned};

proptest! {
    #[test]
    fn toon_output_is_never_a_regression_uniform_arrays(value in common::arb_uniform_array()) {
        let bytes = common::to_json_bytes(&value);
        let opts = ConversionOptions::default();
        let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error for payload-driven input");

        if let Conversion::Toon { report, .. } = result {
            prop_assert!(report.toon_bytes < report.json_bytes);
        }
    }

    #[test]
    fn toon_output_is_never_a_regression_general_json(value in common::arb_json_value()) {
        let bytes = common::to_json_bytes(&value);
        let opts = ConversionOptions::default();
        let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error for payload-driven input");

        if let Conversion::Toon { report, .. } = result {
            prop_assert!(report.toon_bytes < report.json_bytes);
        }
    }
}
