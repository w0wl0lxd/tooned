// SPDX-License-Identifier: AGPL-3.0-only

//! TOON encode/decode wrapper.

use serde_json::Value;
use tooned_types::{ConversionOptions, ToonedError};

/// Conservative nesting-depth guard applied before TOON bytes reach the
/// decoder. Mirrors the JSON guard in `tooned-parse`.
const MAX_STRUCTURAL_DEPTH: usize = 100;

/// Flat, iterative (non-recursive) walk that rejects input nested deeper
/// than the safe structural-depth limit worth of `{`/`[`/`}`/`]`, ignoring bracket
/// characters inside double-quoted strings (with `\"` escape handling).
fn exceeds_max_structural_depth(input: &[u8]) -> bool {
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut escaped = false;
    for &b in input {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > MAX_STRUCTURAL_DEPTH {
                    return true;
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}

/// Encodes a `serde_json::Value` into TOON format.
pub fn encode_toon(value: &Value) -> Result<String, ToonedError> {
    toon_lsp::toon::encode(value).map_err(|e| ToonedError::DecodeFailed(e.to_string()))
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
    if text.len() > ConversionOptions::default().max_input_bytes {
        return Err(ToonedError::InputTooLarge);
    }
    if exceeds_max_structural_depth(text.as_bytes()) {
        return Err(ToonedError::DecodeFailed(
            "input nesting exceeds the safe structural-depth limit".to_string(),
        ));
    }
    toon_lsp::toon::decode(text).map_err(|e| ToonedError::DecodeFailed(e.to_string()))
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
}
