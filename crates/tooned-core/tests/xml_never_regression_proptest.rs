//! T080-T081: XML never-a-regression property tests.
//!
//! For every generated XML record-list payload where `maybe_tooned` returns
//! `Conversion::Toon`, `report.toon_bytes` must be strictly less than
//! `report.json_bytes`.

mod common;

use proptest::prelude::*;
use tooned_core::{Conversion, ConversionOptions, DocType, maybe_tooned};

proptest! {
    #[test]
    fn xml_toon_output_is_never_a_regression(
        (bytes, _expected) in common::xml::arb_xml_record_list(),
    ) {
        let opts = ConversionOptions {
            format_hint: Some(DocType::Xml),
            ..ConversionOptions::default()
        };
        let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error for XML input");

        if let Conversion::Toon { report, .. } = result {
            prop_assert!(report.toon_bytes < report.json_bytes);
        }
    }
}
