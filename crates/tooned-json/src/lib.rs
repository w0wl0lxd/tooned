// SPDX-License-Identifier: AGPL-3.0-only

//! JSON/NDJSON parsing.

use serde_json::Value;
use tooned_parse::{ParseError, exceeds_max_structural_depth};

/// Threshold (bytes) above which JSON parsing prefers the SIMD-accelerated
/// `sonic-rs` fast path over `serde_json`, on x86_64/aarch64. Chosen as a
/// conservative starting point per research.md #4 ("exact threshold to be
/// tuned during implementation via benchmarking, not fixed at planning
/// time"): below this, `serde_json`'s lower setup overhead tends to win;
/// above it, SIMD parsing has enough bytes to pay for itself. Revisit with
/// `criterion` benchmarks before v1 ships (Polish phase, T077).
pub const SONIC_RS_THRESHOLD_BYTES: usize = 8 * 1024;

/// Parses JSON input into a `serde_json::Value`.
pub fn parse_json(input: &[u8]) -> Result<Value, ParseError> {
    if exceeds_max_structural_depth(input) {
        return Err(ParseError::TooDeep);
    }
    if use_simd_json(input.len()) {
        sonic_rs::from_slice::<Value>(input).map_err(|e| ParseError::Json(e.to_string()))
    } else {
        serde_json::from_slice::<Value>(input).map_err(|e| ParseError::Json(e.to_string()))
    }
}

/// Parses NDJSON input into a `serde_json::Value` (as an array).
pub fn parse_ndjson(input: &[u8]) -> Result<Value, ParseError> {
    #[allow(clippy::naive_bytecount)]
    let estimated_lines = input.iter().filter(|&&b| b == b'\n').count() + 1;
    let mut items = Vec::with_capacity(estimated_lines);
    for line in input.split(|b| *b == b'\n') {
        let trimmed = line.trim_ascii();
        if trimmed.is_empty() {
            continue;
        }
        if exceeds_max_structural_depth(trimmed) {
            return Err(ParseError::TooDeep);
        }
        let value = serde_json::from_slice::<Value>(trimmed)
            .map_err(|e| ParseError::Json(e.to_string()))?;
        items.push(value);
    }
    Ok(Value::Array(items))
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn use_simd_json(len: usize) -> bool {
    len >= SONIC_RS_THRESHOLD_BYTES
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn use_simd_json(_len: usize) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_object() {
        let value = parse_json(br#"{"a": 1, "b": [1, 2]}"#).expect("valid JSON");
        assert_eq!(value, serde_json::json!({"a": 1, "b": [1, 2]}));
    }

    #[test]
    fn parses_ndjson_into_array() {
        let value = parse_ndjson(b"{\"a\":1}\n{\"a\":2}\n").expect("valid NDJSON");
        assert_eq!(value, serde_json::json!([{"a": 1}, {"a": 2}]));
    }

    #[test]
    fn adversarially_deep_json_over_the_sonic_rs_threshold_errors_not_crashes() {
        // Regression test for a real stack-overflow finding: sonic-rs's
        // `Value` deserializer has no recursion-depth guard of its own
        // (unlike serde_json/serde_yaml/toml), so deeply nested JSON large
        // enough to cross SONIC_RS_THRESHOLD_BYTES must be intercepted by
        // `exceeds_max_structural_depth` *before* reaching sonic_rs, or the
        // process aborts on a stack overflow -- not something a `Result`
        // or `catch_unwind` can catch after the fact.
        let depth = 10_000;
        let mut bytes = Vec::with_capacity(depth * 2 + SONIC_RS_THRESHOLD_BYTES);
        bytes.extend(std::iter::repeat_n(b'[', depth));
        bytes.extend(std::iter::repeat_n(b']', depth));
        assert!(bytes.len() >= SONIC_RS_THRESHOLD_BYTES);
        let result = parse_json(&bytes);
        assert!(matches!(result, Err(ParseError::TooDeep)));
    }

    #[test]
    fn sonic_rs_fast_path_matches_serde_json_for_a_large_payload() {
        let mut s = String::from("[");
        for i in 0..2000 {
            if i > 0 {
                s.push(',');
            }
            let _ = std::fmt::write(&mut s, format_args!(r#"{{"id":{i}}}"#));
        }
        s.push(']');
        let bytes = s.into_bytes();
        assert!(bytes.len() >= SONIC_RS_THRESHOLD_BYTES);
        let via_fast_path = parse_json(&bytes).expect("valid JSON");
        let via_serde_json: Value = serde_json::from_slice(&bytes).expect("valid JSON");
        assert_eq!(via_fast_path, via_serde_json);
    }
}
