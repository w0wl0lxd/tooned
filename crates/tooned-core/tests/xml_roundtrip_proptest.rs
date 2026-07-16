//! T080-T081: XML round-trip fidelity property tests.
//!
//! For every generated XML record-list payload where `maybe_tooned` returns
//! `Conversion::Toon`, `decode_toon(&text)` must succeed and be equal to the
//! `Value` produced by `tooned_core::xml::parse` for the same input.

mod common;

use proptest::prelude::*;
use tooned_core::{Conversion, ConversionOptions, DocType, decode_toon, maybe_tooned};

proptest! {
    #[test]
    fn xml_toon_output_round_trips_to_parsed_value(
        (bytes, expected) in common::xml::arb_xml_record_list(),
    ) {
        let opts = ConversionOptions {
            format_hint: Some(DocType::Xml),
            ..ConversionOptions::default()
        };
        let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error for XML input");

        if let Conversion::Toon { text, .. } = result {
            let decoded = decode_toon(&text).expect("a Conversion::Toon's own text must decode");
            prop_assert_eq!(decoded, expected);
        }
    }
}
