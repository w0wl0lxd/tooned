//! `ToonedError` and the `decode_toon` entrypoint
//! (`contracts/tooned-core-api.md`).

use serde_json::Value;

use crate::ConversionOptions;
use crate::parse::exceeds_max_structural_depth;

/// Reserved for genuine caller misuse or explicit decode failures -- never
/// returned by `maybe_tooned`/`inspect` for payload-driven failure
/// (malformed/oversized/ambiguous input), which always resolves to
/// `Conversion::Passthrough` instead (constitution Principle I,
/// `contracts/tooned-core-api.md` preconditions).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ToonedError {
    /// Input exceeded a caller-declared size limit. Not actually returned by
    /// `maybe_tooned`/`inspect` (those downgrade to
    /// `Passthrough { reason: InputTooLarge }` instead, per FR-006); kept as
    /// a variant for callers that want to pre-validate a size limit
    /// themselves before calling in.
    #[error("input exceeds max_input_bytes limit")]
    InputTooLarge,
    /// `decode_toon` failed because `text` is not valid TOON.
    #[error("failed to decode TOON input: {0}")]
    DecodeFailed(String),
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
