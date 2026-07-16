// SPDX-License-Identifier: AGPL-3.0-only

//! T078: `proptest` coverage for CSV/TSV and YAML/TOML detection+conversion
//! parity, beyond the JSON-only Foundational-phase property tests
//! (`roundtrip_proptest.rs`/`never_regression_proptest.rs`/
//! `no_panic_proptest.rs` all serialize exclusively through
//! `serde_json::to_vec`).
//!
//! Two flavors of property, per doctype:
//! - **round-trip + never-regression**, mirroring the Foundational-phase
//!   properties but sourced from CSV/TSV/YAML/TOML text instead of JSON.
//! - **cross-format parity**: the exact same abstract value, serialized
//!   once to JSON and once to the doctype under test, must produce the same
//!   `Conversion` decision (`Toon` vs `Passthrough`) and -- when `Toon` --
//!   decode back to the identical value either way. This is the property
//!   that actually justifies "detection+conversion parity" in the task
//!   description: it's not enough for each doctype to round-trip in
//!   isolation, the *decision* itself must not depend on which source
//!   format produced an equivalent value.
//!
//! Every input here uses an explicit `format_hint` rather than relying on
//! content-sniffing (`detect.rs` already has its own dedicated sniffing
//! unit tests) -- these properties are scoped to `parse`/`convert`, not
//! `detect`.
//!
//! `#[allow(clippy::expect_used)]`: this whole file is test-only helper
//! code in an integration-test binary; none of these helper functions are
//! themselves `#[test]`-attributed or `cfg(test)`-scoped (they're called
//! from `proptest!`'s generated closures instead), so clippy's
//! `allow-expect-in-tests` config doesn't recognize them as test code even
//! though they unambiguously are.
#![allow(clippy::expect_used)]

mod common;

use std::io::Write as _;

use proptest::prelude::*;
use serde_json::{Map, Value};
use tooned_core::{Conversion, ConversionOptions, DocType, decode_toon, maybe_tooned};

// ---------------------------------------------------------------------
// CSV/TSV: a uniform array of objects, all-string field values (CSV/TSV
// have no native types -- every field tooned-core parses back is a
// `Value::String`, per `parse.rs`'s `parse_delimited`).
// ---------------------------------------------------------------------

/// A CSV/TSV-safe field value: printable ASCII, deliberately including
/// characters (`,`, `"`, whitespace) that need real quoting/escaping --
/// serialized via the `csv` crate's own writer (not hand-rolled), so
/// escaping correctness is never the thing under test here.
fn arb_field_value() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_ ,\"]{0,10}"
}

/// `rows` records sharing the same `keys` header, each field an
/// `arb_field_value()`. Returns the header + rows as `Vec<Vec<String>>`
/// (header first) ready to feed to a `csv::Writer`.
fn arb_delimited_table() -> impl Strategy<Value = Vec<Vec<String>>> {
    proptest::collection::vec("[a-z]{1,8}", 1..5usize).prop_flat_map(|keys| {
        let row = proptest::collection::vec(arb_field_value(), keys.len());
        proptest::collection::vec(row, 1..10usize).prop_map(move |rows| {
            let mut table = vec![keys.clone()];
            table.extend(rows);
            table
        })
    })
}

/// Writes `table` (header + rows) as delimited text via the `csv` crate's
/// own writer -- guarantees valid quoting/escaping for any field content.
fn write_delimited(table: &[Vec<String>], delimiter: u8) -> Vec<u8> {
    let mut writer = csv::WriterBuilder::new().delimiter(delimiter).from_writer(Vec::new());
    for row in table {
        writer.write_record(row).expect("writing a CSV/TSV record must not fail for ASCII fields");
    }
    writer.flush().expect("flushing the CSV/TSV writer must not fail");
    writer.into_inner().expect("into_inner must not fail after a successful flush")
}

/// The `Value` tooned-core's own delimited parser would produce for
/// `table` -- computed independently here (not by calling into
/// `tooned_core`'s private `parse` module, which integration tests can't
/// reach) so this is a genuine external check, not a tautology.
fn expected_delimited_value(table: &[Vec<String>]) -> Value {
    let (header, rows) = table.split_first().expect("arb_delimited_table always has a header");
    Value::Array(
        rows.iter()
            .map(|row| {
                let mut map = Map::new();
                for (key, field) in header.iter().zip(row.iter()) {
                    map.insert(key.clone(), Value::String(field.clone()));
                }
                Value::Object(map)
            })
            .collect(),
    )
}

fn check_delimited_round_trip(
    table: &[Vec<String>],
    doc_type: DocType,
    delimiter: u8,
) -> Result<(), TestCaseError> {
    let bytes = write_delimited(table, delimiter);
    let expected = expected_delimited_value(table);
    let opts = ConversionOptions { format_hint: Some(doc_type), ..ConversionOptions::default() };

    let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error");
    if let Conversion::Toon { text, report } = result {
        // Never-regression.
        prop_assert!(report.toon_bytes < report.json_bytes);
        // Round-trip fidelity against the independently-computed expected
        // value, not just "decodes to *something*".
        let decoded = decode_toon(&text).expect("a Conversion::Toon's own text must decode");
        prop_assert_eq!(decoded, expected);
    }
    Ok(())
}

proptest! {
    #[test]
    fn csv_round_trips_and_never_regresses(table in arb_delimited_table()) {
        check_delimited_round_trip(&table, DocType::Csv, b',')?;
    }

    #[test]
    fn tsv_round_trips_and_never_regresses(table in arb_delimited_table()) {
        check_delimited_round_trip(&table, DocType::Tsv, b'\t')?;
    }
}

// ---------------------------------------------------------------------
// YAML: reuses the Foundational phase's JSON-value generators (already
// float-free, per `common`'s doc comment on the known TOON whole-number-
// float round-trip quirk) but serializes through `serde_yaml` instead.
// ---------------------------------------------------------------------

fn to_yaml_bytes(value: &Value) -> Vec<u8> {
    serde_yaml::to_string(value)
        .expect("arb_json_value/arb_uniform_array must serialize to YAML")
        .into_bytes()
}

proptest! {
    #[test]
    fn yaml_round_trips_and_never_regresses(value in common::arb_uniform_array()) {
        let bytes = to_yaml_bytes(&value);
        let opts = ConversionOptions { format_hint: Some(DocType::Yaml), ..ConversionOptions::default() };
        let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error");
        if let Conversion::Toon { text, report } = result {
            prop_assert!(report.toon_bytes < report.json_bytes);
            let decoded = decode_toon(&text).expect("a Conversion::Toon's own text must decode");
            prop_assert_eq!(decoded, value);
        }
    }

    /// Cross-format parity: the same abstract value, sourced from JSON vs
    /// YAML, must reach the same `Toon`-vs-`Passthrough` decision and
    /// (when `Toon`) decode back to the identical value either way.
    #[test]
    fn yaml_and_json_reach_the_same_conversion_decision_for_the_same_value(value in common::arb_uniform_array()) {
        let json_bytes = common::to_json_bytes(&value);
        let yaml_bytes = to_yaml_bytes(&value);
        let json_opts = ConversionOptions { format_hint: Some(DocType::Json), ..ConversionOptions::default() };
        let yaml_opts = ConversionOptions { format_hint: Some(DocType::Yaml), ..ConversionOptions::default() };

        let from_json = maybe_tooned(&json_bytes, &json_opts).expect("maybe_tooned must not error");
        let from_yaml = maybe_tooned(&yaml_bytes, &yaml_opts).expect("maybe_tooned must not error");

        match (from_json, from_yaml) {
            (Conversion::Toon { text: json_text, .. }, Conversion::Toon { text: yaml_text, .. }) => {
                let decoded_from_json = decode_toon(&json_text).expect("valid TOON");
                let decoded_from_yaml = decode_toon(&yaml_text).expect("valid TOON");
                prop_assert_eq!(decoded_from_json, decoded_from_yaml);
            }
            (Conversion::Passthrough { .. }, Conversion::Passthrough { .. }) => {}
            (json_result, yaml_result) => {
                prop_assert!(
                    false,
                    "JSON and YAML sources for the same value reached different decisions: \
                     json={json_result:?}, yaml={yaml_result:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------
// TOML: top-level must be a table (object) and has no `null` -- a
// dedicated, TOML-safe value generator rather than reusing the JSON ones.
// ---------------------------------------------------------------------

/// A TOML-representable scalar: no `null` (TOML has no null type).
fn arb_toml_scalar() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<bool>().prop_map(Value::Bool),
        (-1_000_000i64..1_000_000).prop_map(Value::from),
        "[a-zA-Z0-9_ ]{0,12}".prop_map(Value::String),
    ]
}

/// A TOML-representable root table: an object whose values are scalars or
/// arrays of scalars sharing a single type each (TOML arrays must be
/// homogeneous) -- deliberately simple/shallow, since the point of this
/// property is doctype parity, not exercising TOML's full nested-table
/// grammar (already covered by `parse.rs`'s own TOML unit tests).
fn arb_toml_root() -> impl Strategy<Value = Value> {
    let field = prop_oneof![
        arb_toml_scalar(),
        proptest::collection::vec(any::<bool>(), 0..4)
            .prop_map(|v| { Value::Array(v.into_iter().map(Value::Bool).collect()) }),
    ];
    proptest::collection::vec(("[a-z]{1,8}", field), 1..6usize).prop_map(|pairs| {
        let mut map = Map::new();
        for (k, v) in pairs {
            map.insert(k, v);
        }
        Value::Object(map)
    })
}

fn to_toml_bytes(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write!(out, "{}", toml::to_string(value).expect("arb_toml_root must serialize to TOML"))
        .expect("writing to a Vec<u8> cannot fail");
    out
}

proptest! {
    #[test]
    fn toml_round_trips_and_never_regresses(value in arb_toml_root()) {
        let bytes = to_toml_bytes(&value);
        let opts = ConversionOptions { format_hint: Some(DocType::Toml), ..ConversionOptions::default() };
        let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error");
        if let Conversion::Toon { text, report } = result {
            prop_assert!(report.toon_bytes < report.json_bytes);
            let decoded = decode_toon(&text).expect("a Conversion::Toon's own text must decode");
            prop_assert_eq!(decoded, value);
        }
    }

    /// Cross-format parity for TOML vs JSON, mirroring the YAML/JSON
    /// property above.
    #[test]
    fn toml_and_json_reach_the_same_conversion_decision_for_the_same_value(value in arb_toml_root()) {
        let json_bytes = common::to_json_bytes(&value);
        let toml_bytes = to_toml_bytes(&value);
        let json_opts = ConversionOptions { format_hint: Some(DocType::Json), ..ConversionOptions::default() };
        let toml_opts = ConversionOptions { format_hint: Some(DocType::Toml), ..ConversionOptions::default() };

        let from_json = maybe_tooned(&json_bytes, &json_opts).expect("maybe_tooned must not error");
        let from_toml = maybe_tooned(&toml_bytes, &toml_opts).expect("maybe_tooned must not error");

        match (from_json, from_toml) {
            (Conversion::Toon { text: json_text, .. }, Conversion::Toon { text: toml_text, .. }) => {
                let decoded_from_json = decode_toon(&json_text).expect("valid TOON");
                let decoded_from_toml = decode_toon(&toml_text).expect("valid TOON");
                prop_assert_eq!(decoded_from_json, decoded_from_toml);
            }
            (Conversion::Passthrough { .. }, Conversion::Passthrough { .. }) => {}
            (json_result, toml_result) => {
                prop_assert!(
                    false,
                    "JSON and TOML sources for the same value reached different decisions: \
                     json={json_result:?}, toml={toml_result:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------
// NDJSON: one JSON value per line -- reuses the Foundational-phase
// uniform-array-of-objects generator (the shape TOON's tabular encoding is
// designed for), but serializes each array element on its own line instead
// of as a single JSON array literal. Regression coverage for the sixth
// explicitly-supported doctype, which previously had no round-trip/
// never-regression/cross-format-parity property test at all -- only
// sniffing (`detect.rs::sniffs_ndjson`) and bare parsing
// (`parse.rs::parses_ndjson_into_array`) were covered, neither of which
// ever calls `maybe_tooned`/`decode_toon`.
// ---------------------------------------------------------------------

/// Writes `value` (always a top-level `Value::Array`, per
/// `arb_uniform_array`) as NDJSON: one compact JSON line per array element,
/// newline-terminated -- exactly the shape `parse.rs::parse_ndjson` expects.
fn to_ndjson_bytes(value: &Value) -> Vec<u8> {
    let items = value.as_array().expect("common::arb_uniform_array always produces an array");
    let mut out = Vec::new();
    for item in items {
        serde_json::to_writer(&mut out, item)
            .expect("common::arb_uniform_array elements must serialize to JSON");
        out.push(b'\n');
    }
    out
}

proptest! {
    #[test]
    fn ndjson_round_trips_and_never_regresses(value in common::arb_uniform_array()) {
        let bytes = to_ndjson_bytes(&value);
        let opts = ConversionOptions { format_hint: Some(DocType::NdJson), ..ConversionOptions::default() };
        let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error");
        if let Conversion::Toon { text, report } = result {
            prop_assert!(report.toon_bytes < report.json_bytes);
            let decoded = decode_toon(&text).expect("a Conversion::Toon's own text must decode");
            prop_assert_eq!(decoded, value);
        }
    }

    /// Cross-format parity: the same abstract value, sourced from JSON vs
    /// NDJSON, must reach the same `Toon`-vs-`Passthrough` decision and
    /// (when `Toon`) decode back to the identical value either way.
    #[test]
    fn ndjson_and_json_reach_the_same_conversion_decision_for_the_same_value(value in common::arb_uniform_array()) {
        let json_bytes = common::to_json_bytes(&value);
        let ndjson_bytes = to_ndjson_bytes(&value);
        let json_opts = ConversionOptions { format_hint: Some(DocType::Json), ..ConversionOptions::default() };
        let ndjson_opts = ConversionOptions { format_hint: Some(DocType::NdJson), ..ConversionOptions::default() };

        let from_json = maybe_tooned(&json_bytes, &json_opts).expect("maybe_tooned must not error");
        let from_ndjson = maybe_tooned(&ndjson_bytes, &ndjson_opts).expect("maybe_tooned must not error");

        match (from_json, from_ndjson) {
            (Conversion::Toon { text: json_text, .. }, Conversion::Toon { text: ndjson_text, .. }) => {
                let decoded_from_json = decode_toon(&json_text).expect("valid TOON");
                let decoded_from_ndjson = decode_toon(&ndjson_text).expect("valid TOON");
                prop_assert_eq!(decoded_from_json, decoded_from_ndjson);
            }
            (Conversion::Passthrough { .. }, Conversion::Passthrough { .. }) => {}
            (json_result, ndjson_result) => {
                prop_assert!(
                    false,
                    "JSON and NDJSON sources for the same value reached different decisions: \
                     json={json_result:?}, ndjson={ndjson_result:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------
// XML: record-list-style XML with repeated child elements and many
// attributes -- the shape `tooned-core::xml::parse` produces a uniform
// array of `@`-prefixed objects from, which TOON's tabular encoding
// compresses well.
// ---------------------------------------------------------------------

proptest! {
    #[test]
    fn xml_round_trips_and_never_regresses((bytes, expected) in common::xml::arb_xml_record_list()) {
        let opts = ConversionOptions { format_hint: Some(DocType::Xml), ..ConversionOptions::default() };
        let result = maybe_tooned(&bytes, &opts).expect("maybe_tooned must not error for XML input");

        if let Conversion::Toon { text, report } = result {
            prop_assert!(report.toon_bytes < report.json_bytes);
            let decoded = decode_toon(&text).expect("a Conversion::Toon's own text must decode");
            prop_assert_eq!(decoded, expected);
        }
    }

    /// Cross-format parity: the same abstract value, sourced from JSON vs
    /// XML, must reach the same `Toon`-vs-`Passthrough` decision and (when
    /// `Toon`) decode back to the identical value either way.
    #[test]
    fn xml_and_json_reach_the_same_conversion_decision_for_the_same_value(
        (xml_bytes, value) in common::xml::arb_xml_record_list(),
    ) {
        let json_bytes = common::to_json_bytes(&value);
        let json_opts = ConversionOptions { format_hint: Some(DocType::Json), ..ConversionOptions::default() };
        let xml_opts = ConversionOptions { format_hint: Some(DocType::Xml), ..ConversionOptions::default() };

        let from_json = maybe_tooned(&json_bytes, &json_opts).expect("maybe_tooned must not error");
        let from_xml = maybe_tooned(&xml_bytes, &xml_opts).expect("maybe_tooned must not error");

        match (from_json, from_xml) {
            (Conversion::Toon { text: json_text, .. }, Conversion::Toon { text: xml_text, .. }) => {
                let decoded_from_json = decode_toon(&json_text).expect("valid TOON");
                let decoded_from_xml = decode_toon(&xml_text).expect("valid TOON");
                prop_assert_eq!(decoded_from_json, decoded_from_xml);
            }
            (Conversion::Passthrough { .. }, Conversion::Passthrough { .. }) => {}
            (json_result, xml_result) => {
                prop_assert!(
                    false,
                    "JSON and XML sources for the same value reached different decisions: \
                     json={json_result:?}, xml={xml_result:?}"
                );
            }
        }
    }
}
