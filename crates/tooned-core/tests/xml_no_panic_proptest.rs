// SPDX-License-Identifier: AGPL-3.0-only

//! T082: XML no-panic property tests.
//!
//! `maybe_tooned` and `inspect` must not panic for arbitrary XML content,
//! invalid UTF-8, HTML-like content, and truncated XML, regardless of whether
//! a `format_hint` is provided.

mod common;

use proptest::prelude::*;
use tooned_core::{ConversionOptions, DocType, inspect, maybe_tooned};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn never_panics_on_xml_record_list((bytes, _value) in common::xml::arb_xml_record_list()) {
        let opts = ConversionOptions::default();
        let _ = maybe_tooned(&bytes, &opts);
        let _ = inspect(&bytes, &opts);

        let hinted_opts = ConversionOptions {
            format_hint: Some(DocType::Xml),
            ..ConversionOptions::default()
        };
        let _ = maybe_tooned(&bytes, &hinted_opts);
        let _ = inspect(&bytes, &hinted_opts);
    }

    #[test]
    fn never_panics_on_arbitrary_bytes_with_xml_hint(
        bytes in proptest::collection::vec(any::<u8>(), 0..2048),
    ) {
        let opts = ConversionOptions {
            format_hint: Some(DocType::Xml),
            ..ConversionOptions::default()
        };
        let _ = maybe_tooned(&bytes, &opts);
        let _ = inspect(&bytes, &opts);
    }

    #[test]
    fn never_panics_on_xml_like_strings_with_xml_hint(
        text in proptest::collection::vec(
            prop_oneof![
                Just("<".to_string()),
                Just(">".to_string()),
                Just("&".to_string()),
                Just("/".to_string()),
                Just("\"".to_string()),
                Just("'".to_string()),
                "[a-zA-Z0-9<>&\"'/ ]{0,32}",
            ],
            0..128usize,
        ),
    ) {
        let bytes: Vec<u8> = text.concat().into_bytes();
        let opts = ConversionOptions {
            format_hint: Some(DocType::Xml),
            ..ConversionOptions::default()
        };
        let _ = maybe_tooned(&bytes, &opts);
        let _ = inspect(&bytes, &opts);
    }
}

#[test]
fn never_panics_on_invalid_utf8_with_xml_hint() {
    let bytes: &[u8] = &[0xFF, 0xFE, 0x00, 0x7B, 0x22, 0xC0, 0x80, 0xED, 0xA0, 0x80];
    let opts =
        ConversionOptions { format_hint: Some(DocType::Xml), ..ConversionOptions::default() };
    let _ = maybe_tooned(bytes, &opts);
    let _ = inspect(bytes, &opts);
}

#[test]
fn never_panics_on_truncated_xml_with_xml_hint() {
    let bytes: &[u8] = b"<?xml version=\"1.0\"?><root attr=\"value\">";
    let opts =
        ConversionOptions { format_hint: Some(DocType::Xml), ..ConversionOptions::default() };
    let _ = maybe_tooned(bytes, &opts);
    let _ = inspect(bytes, &opts);
}

#[test]
fn never_panics_on_html_like_with_xml_hint() {
    let bytes: &[u8] = b"<!DOCTYPE html><html><body><p>hello</p></body></html>";
    let opts =
        ConversionOptions { format_hint: Some(DocType::Xml), ..ConversionOptions::default() };
    let _ = maybe_tooned(bytes, &opts);
    let _ = inspect(bytes, &opts);
}
