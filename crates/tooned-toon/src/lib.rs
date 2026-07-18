// SPDX-License-Identifier: AGPL-3.0-only

//! TOON encode/decode wrapper.

pub mod dict;
pub use dict::{apply_dict, expand_legend};

use serde_json::Value;
use tooned_parse::exceeds_max_structural_depth;
use tooned_types::{ConversionOptions, ToonedError};

/// Encodes a `serde_json::Value` into TOON format.
///
/// Unlike the thin upstream codec it wraps, this function enforces the same
/// round-trip fidelity guarantee the conversion pipeline relies on
/// (`attempt`/`maybe_onto`/`maybe_tron` all assert `decode(encode(x)) == x`):
/// after encoding it decodes the result and compares it to the original
/// `value`, returning [`ToonedError::DecodeFailed`] when they differ. This
/// matters because the upstream `toon_lsp` encoder normalizes numeric literals
/// (e.g. `1.0` -> `1`, `-0.0` -> `0`), which silently drops the int/float
/// distinction and the negative-zero sign. The `attempt`-level gate catches
/// the int/float case for the main pipeline, but any direct caller of
/// `encode_toon` (or a future MCP `tooned_encode` tool) would otherwise ship
/// the lossy encoding. Failing closed here means a lossy value is surfaced as
/// an error for the caller to handle (e.g. fall back to a passthrough) rather
/// than emitted as corrupt TOON.
pub fn encode_toon(value: &Value) -> Result<String, ToonedError> {
    let encoded =
        toon_lsp::toon::encode(value).map_err(|e| ToonedError::DecodeFailed(e.to_string()))?;
    let round_trip_ok =
        match decode_toon_with_limit(&encoded, ConversionOptions::default().max_input_bytes) {
            Ok(decoded) => decoded == *value,
            // A decode failure means the encoding is not faithfully reversible, so
            // it is not lossless -- fail closed (refuse to emit).
            Err(_) => false,
        };
    if !round_trip_ok {
        return Err(ToonedError::DecodeFailed(
            "TOON encoding is not lossless for this value (numeric type / negative-zero \
             normalization); refusing to emit a corrupt encoding"
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
    let mut out = String::new();
    toon_lsp::toon::encode_into(value, &toon_lsp::toon::ToonConfig::default(), &mut out)
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
/// and the same structural-depth limit *before* `toon_lsp::toon::decode` (an
/// external deserializer whose own recursion-depth behavior on adversarial
/// input isn't guaranteed) ever sees it -- both public callers of this
/// function (`tooned convert --to json`, the MCP `tooned_decode` tool)
/// pass caller-supplied text with no upstream size/depth validation of their
/// own.
///
/// # Errors
/// Returns [`ToonedError::InputTooLarge`] when `text` exceeds the default
/// `max_input_bytes` cap, [`ToonedError::DecodeFailed`] when `text` exceeds
/// the safe structural-nesting depth or is otherwise not valid TOON.
pub fn decode_toon(text: &str) -> Result<Value, ToonedError> {
    decode_toon_with_limit(text, ConversionOptions::default().max_input_bytes)
}

/// Decodes a TOON document with a caller-supplied byte-size cap.
///
/// This is used by `tooned-convert`'s internal round-trip check so an
/// input that passed the caller's `max_input_bytes` is not rejected again
/// by the default 2 MiB cap when the TOON text is larger than the original
/// JSON (TOON is normally smaller, but callers can raise the cap above the
/// default).
pub fn decode_toon_with_limit(text: &str, max_input_bytes: usize) -> Result<Value, ToonedError> {
    // Reverse any dictionary `legend:` block before the external codec ever
    // sees the text. The legend is purely a tooned addition layered on top of
    // standard TOON, so `toon-lsp` must receive plain TOON. Enforce the
    // caller's byte cap both before and during expansion.
    if text.len() > max_input_bytes {
        return Err(ToonedError::InputTooLarge);
    }
    let text = expand_legend(text, max_input_bytes)?;
    if exceeds_max_structural_depth(text.as_bytes()) {
        return Err(ToonedError::DecodeFailed(
            "input nesting exceeds the safe structural-depth limit".to_string(),
        ));
    }
    toon_lsp::toon::decode(&text).map_err(|e| ToonedError::DecodeFailed(e.to_string()))
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
    fn encode_toon_rejects_lossy_numeric_values() {
        // The upstream codec normalizes whole-number floats to integers and
        // collapses negative zero to zero, which would silently drop the
        // int/float distinction and the negative-zero sign. encode_toon must
        // refuse to emit a corrupt encoding rather than ship it.
        assert!(encode_toon(&serde_json::json!({"x": 1.0})).is_err());
        assert!(encode_toon(&serde_json::json!({"x": -0.0})).is_err());
        // A genuinely lossless value still encodes successfully.
        assert!(encode_toon(&serde_json::json!({"x": 1})).is_ok());
        assert!(encode_toon(&serde_json::json!({"x": 1.5})).is_ok());
    }
}
