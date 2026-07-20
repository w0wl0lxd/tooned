// SPDX-License-Identifier: AGPL-3.0-only

//! TOON encode/decode wrapper.
//!
//! The underlying codec lives in [`toon_lsp::toon`]; this crate layers on the
//! lossless dictionary tier and a fail-closed round-trip gate used by the
//! conversion pipeline.

pub mod dict;
pub use dict::{apply_dict, expand_legend};

use serde_json::Value;
use toon_lsp::toon::{Delimiter, ToonConfig, decode_with_config, encode_into};
use tooned_parse::exceeds_max_structural_depth;
use tooned_types::{ConversionOptions, ToonedError};

/// Build a [`ToonConfig`] from [`ConversionOptions`].
///
/// The tooned defaults prefer compact nested-object output while remaining
/// lossless: key folding is on, path expansion is on so the inverse decode
/// reconstructs the original structure, and numeric types are preserved.
#[must_use]
pub fn toon_config(opts: &ConversionOptions) -> ToonConfig {
    ToonConfig {
        indent: 2,
        delimiter: Delimiter::Comma,
        fold_keys: opts.fold_keys,
        flatten_keys: opts.flatten_keys,
        expand_paths: opts.expand_paths,
        preserve_number_types: opts.preserve_number_types,
    }
}

/// Encodes a `serde_json::Value` into TOON format using the default options.
///
/// Enforces the round-trip fidelity guarantee the conversion pipeline relies on
/// (`attempt`/`maybe_onto`/`maybe_tron` all assert `decode(encode(x)) == x`):
/// after encoding it decodes the result and compares it to the original
/// `value`, returning [`ToonedError::DecodeFailed`] when they differ.
pub fn encode_toon(value: &Value) -> Result<String, ToonedError> {
    encode_toon_with_options(value, &ConversionOptions::default())
}

/// Encodes a `serde_json::Value` into TOON format with caller-supplied options.
///
/// Fails closed when the result does not round-trip under the same options.
pub fn encode_toon_with_options(
    value: &Value,
    opts: &ConversionOptions,
) -> Result<String, ToonedError> {
    let encoded = encode_toon_raw_with_options(value, opts)?;
    let round_trip_ok = match decode_toon_with_options(&encoded, opts) {
        Ok(decoded) => decoded == *value,
        // A decode failure means the encoding is not faithfully reversible, so
        // it is not lossless -- fail closed (refuse to emit).
        Err(_) => false,
    };
    if !round_trip_ok {
        return Err(ToonedError::DecodeFailed(
            "TOON encoding is not lossless for this value; refusing to emit a corrupt encoding"
                .to_string(),
        ));
    }
    Ok(encoded)
}

/// Raw TOON encode with no round-trip fidelity check.
///
/// This is the thin upstream-codec wrapper that the conversion pipeline
/// (`attempt`/`maybe_onto`/`maybe_tron`) uses internally: those callers apply
/// their own strict `decode(encode(x)) == x` gate (and fall back to a
/// passthrough on mismatch), so they do not need `encode_toon`'s extra guard.
/// Direct callers that do NOT perform their own round-trip check should use
/// [`encode_toon`] instead, which fails closed on lossy values.
pub fn encode_toon_raw(value: &Value) -> Result<String, ToonedError> {
    encode_toon_raw_with_options(value, &ConversionOptions::default())
}

/// Raw TOON encode with caller-supplied options and no round-trip fidelity check.
pub fn encode_toon_raw_with_options(
    value: &Value,
    opts: &ConversionOptions,
) -> Result<String, ToonedError> {
    let mut out = String::new();
    encode_into(value, &toon_config(opts), &mut out)
        .map_err(|e| ToonedError::DecodeFailed(e.to_string()))?;
    Ok(out)
}

/// Decodes a TOON document back into a structured [`serde_json::Value`].
/// Used by `tooned convert --to json` and the MCP `tooned_decode` tool.
///
/// Guarded exactly like the encode-direction parse path
/// (`contracts/tooned-core-api.md`'s "max_input_bytes cap ... enforced
/// before any parsing is attempted" applies here too): `text` is checked
/// against the same default `max_input_bytes` cap ([`ConversionOptions::default`])
/// and the same structural-depth limit *before* the external codec ever sees
/// it -- both public callers of this function (`tooned convert --to json`, the
/// MCP `tooned_decode` tool) pass caller-supplied text with no upstream size/depth
/// validation of their own.
///
/// # Errors
/// Returns [`ToonedError::InputTooLarge`] when `text` exceeds the default
/// `max_input_bytes` cap, [`ToonedError::DecodeFailed`] when `text` exceeds
/// the safe structural-nesting depth or is otherwise not valid TOON.
pub fn decode_toon(text: &str) -> Result<Value, ToonedError> {
    decode_toon_with_options(text, &ConversionOptions::default())
}

/// Decodes a TOON document using caller-supplied options.
pub fn decode_toon_with_options(
    text: &str,
    opts: &ConversionOptions,
) -> Result<Value, ToonedError> {
    if text.len() > opts.max_input_bytes {
        return Err(ToonedError::InputTooLarge);
    }
    // Reverse any dictionary `legend:` block before the external codec ever
    // sees the text. The legend is purely a tooned addition layered on top of
    // standard TOON, so `toon-lsp` must receive plain TOON.
    let text = expand_legend(text, opts.max_input_bytes)?;
    if exceeds_max_structural_depth(text.as_bytes()) {
        return Err(ToonedError::DecodeFailed(
            "input nesting exceeds the safe structural-depth limit".to_string(),
        ));
    }
    decode_with_config(&text, &toon_config(opts))
        .map_err(|e| ToonedError::DecodeFailed(e.to_string()))
}

/// Fast-path decode used by the conversion pipeline's round-trip fidelity check.
///
/// Unlike [`decode_toon_with_options`], this skips the two guards that are
/// redundant *for text the pipeline itself just produced*:
///
/// 1. The dictionary `legend:` expansion -- the caller knows whether
///    `apply_dict` introduced a legend (when it did not, `expand_legend`
///    would still allocate a full copy of the whole document for nothing).
/// 2. The structural-depth re-scan -- the source `Value` was already validated
///    for depth by the format parser before it was ever encoded, and the
///    encoded TOON's nesting depth equals that value's, so re-checking here is
///    pure redundancy.
///
/// It still calls the external codec, which performs its own `remove_block_comments`
/// normalization; that pass is required and cannot be skipped. The result is
/// compared against the original `Value` by the caller to enforce losslessness.
pub fn decode_toon_raw_with_options(
    text: &str,
    opts: &ConversionOptions,
) -> Result<Value, ToonedError> {
    decode_with_config(text, &toon_config(opts))
        .map_err(|e| ToonedError::DecodeFailed(e.to_string()))
}

/// Decodes a TOON document with a caller-supplied byte-size cap.
///
/// This is used by `tooned-convert`'s internal round-trip check so an
/// input that passed the caller's `max_input_bytes` is not rejected again
/// by the default 2 MiB cap when the TOON text is larger than the original
/// JSON (TOON is normally smaller, but callers can raise the cap above the
/// default).
pub fn decode_toon_with_limit(text: &str, max_input_bytes: usize) -> Result<Value, ToonedError> {
    decode_toon_with_options(
        text,
        &ConversionOptions { max_input_bytes, ..ConversionOptions::default() },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_valid_toon() {
        let toon = "a: 1\nb: hello\n";
        let value = decode_toon(toon).expect("valid TOON must decode");
        assert_eq!(value, serde_json::json!({"a": 1, "b": "hello"}));
    }

    #[test]
    fn reports_decode_failure_without_panicking() {
        // A lone unterminated quoted string is a genuine TOON syntax error
        // (unlike arbitrary prose, which TOON's lenient scalar grammar will
        // happily accept as a bare top-level string).
        let result = decode_toon("a: \"unterminated string");
        assert!(matches!(result, Err(ToonedError::DecodeFailed(_))));
    }

    #[test]
    fn encode_toon_preserves_numeric_types() {
        // Whole-number floats and negative zero round-trip with their original
        // serde_json::Number type, so encode_toon accepts them.
        assert!(encode_toon(&serde_json::json!({"x": 1.0})).is_ok());
        assert!(encode_toon(&serde_json::json!({"x": -0.0})).is_ok());
        assert!(encode_toon(&serde_json::json!({"x": 1})).is_ok());
        assert!(encode_toon(&serde_json::json!({"x": 1.5})).is_ok());
    }

    #[test]
    fn folded_keys_round_trip() {
        let value = serde_json::json!({"user": {"profile": {"name": "Alice"}}, "tags": ["x", "y"]});
        let toon = encode_toon(&value).expect("folded encode must round-trip");
        let decoded = decode_toon(&toon).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn ecommerce_subset_round_trips() {
        let value = serde_json::json!({
            "order_id": "ORD-1001",
            "customer": "customer_1@example.com",
            "status": "pending",
            "items": [
                {"sku": "SKU-1010", "qty": 1, "price": 11.0},
                {"sku": "SKU-1011", "qty": 2, "price": 11.5}
            ]
        });
        let toon = encode_toon_raw(&value).unwrap();
        eprintln!("TOON:\n{toon}");
        let decoded = decode_toon(&toon).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn full_ecommerce_round_trips() {
        let text = std::fs::read_to_string("tests/fixtures/ecommerce_orders.json").unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let toon = encode_toon_raw(&value).unwrap();
        let decoded = decode_toon(&toon).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn full_ecommerce_round_trips_with_dict() {
        use crate::dict::apply_dict;

        let text = std::fs::read_to_string("tests/fixtures/ecommerce_orders.json").unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let toon = encode_toon_raw(&value).unwrap();
        let dict_toon = match apply_dict(&toon, &[]) {
            Some(d) => d,
            None => toon,
        };
        eprintln!("DICT TOON:\n{dict_toon}");
        let decoded = decode_toon(&dict_toon).unwrap();
        assert_eq!(decoded, value);
    }
}
