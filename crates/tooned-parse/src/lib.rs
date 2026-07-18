// SPDX-License-Identifier: AGPL-3.0-only

//! Shared parsing utilities and error type.

use thiserror::Error;

/// Parse error type shared across format parsers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ParseError {
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("invalid YAML: {0}")]
    Yaml(String),
    #[error("invalid TOML: {0}")]
    Toml(String),
    #[error("invalid CSV/TSV: {0}")]
    Csv(String),
    #[error("invalid XML: {0}")]
    Xml(String),
    #[error("invalid MessagePack: {0}")]
    Msgpack(String),
    #[error("invalid CBOR: {0}")]
    Cbor(String),
    #[error("invalid JSON5: {0}")]
    Json5(String),
    #[error("input is not valid UTF-8")]
    Utf8,
    #[error("input nesting exceeds the safe structural-depth limit")]
    TooDeep,
}

/// Conservative nesting-depth guard applied before JSON bytes reach a real
/// deserializer. `serde_json`'s own `Value` deserializer defaults to
/// rejecting recursion past depth ~127, but **`sonic-rs`'s deserializer does
/// not** -- verified empirically (see module tests below and the T009
/// no-panic property test): adversarially deep bracket nesting fed through
/// `sonic_rs::from_slice::<serde_json::Value>` overflows the stack rather
/// than returning an `Err` once past roughly depth 150-200 on a 2 MiB
/// thread stack (the default per-test-thread stack size `cargo test`/
/// `cargo nextest` use) -- a fatal, *uncatchable* process abort, not a
/// panic, so it cannot be guarded against after the fact. This scan runs
/// ahead of the JSON/NDJSON paths (not YAML/TOML, whose parsers have their
/// own recursion limits and whose quoted/comment text can legitimately
/// contain unbalanced brackets). It stays well under the 150-200 boundary
/// with a wide safety margin (also protects the *subsequent* recursive
/// operations on a successfully-parsed `Value` -- encode/serialize/Drop --
/// which have the same recursion-depth-proportional-to-value-nesting shape).
const MAX_STRUCTURAL_DEPTH: usize = 100;

/// Flat, iterative (non-recursive) walk that rejects input nested deeper
/// than the safe structural-depth limit worth of `{`/`[`/`}`/`]`, ignoring bracket
/// characters inside double-quoted strings (with `\"` escape handling).
/// Only JSON and NDJSON use this guard; YAML and TOML are excluded because
/// their parsers already enforce safe recursion limits and their quoted
/// strings/comments can contain brackets that would produce false positives
/// here.
pub fn exceeds_max_structural_depth(input: &[u8]) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_depth_guard_ignores_brackets_inside_strings() {
        let json = br#"{"a": "[[[[[[[[[[[[[[[[[[[[[[[["}"#;
        assert!(!exceeds_max_structural_depth(json));
    }

    #[test]
    fn adversarially_deep_json_is_rejected() {
        let depth = 10_000;
        let mut bytes = Vec::with_capacity(depth * 2);
        bytes.extend(std::iter::repeat_n(b'[', depth));
        bytes.extend(std::iter::repeat_n(b']', depth));
        assert!(exceeds_max_structural_depth(&bytes));
    }
}
