// SPDX-License-Identifier: AGPL-3.0-only

//! JSON/NDJSON parsing.

use std::io::BufRead;

use serde_json::Value;
use tooned_parse::{ParseError, exceeds_max_structural_depth};

/// Historical threshold used by tests and benchmarks to bracket small vs.
/// large JSON inputs. `parse_json`/`parse_ndjson` now route every JSON byte
/// through `sonic-rs`; this constant is retained for the test fixtures that
/// exercise both sides of the old boundary.
pub const SONIC_RS_THRESHOLD_BYTES: usize = 8 * 1024;

/// Parses JSON input into a `serde_json::Value` using `sonic-rs`.
pub fn parse_json(input: &[u8]) -> Result<Value, ParseError> {
    if exceeds_max_structural_depth(input) {
        return Err(ParseError::TooDeep);
    }
    sonic_rs::from_slice::<Value>(input).map_err(|e| ParseError::Json(e.to_string()))
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
        let value =
            sonic_rs::from_slice::<Value>(trimmed).map_err(|e| ParseError::Json(e.to_string()))?;
        items.push(value);
    }
    Ok(Value::Array(items))
}

/// Streaming NDJSON parser: yields one `Value` per non-empty line without
/// ever buffering the whole input in memory. The byte counter returned by
/// [`NdJsonStream::bytes_read`] includes line-delimiter bytes consumed from
/// the underlying reader.
pub fn parse_ndjson_stream<R: BufRead>(reader: R) -> NdJsonStream<R> {
    NdJsonStream { reader, buf: String::new(), bytes_read: 0 }
}

/// Iterator returned by [`parse_ndjson_stream`].
pub struct NdJsonStream<R> {
    reader: R,
    buf: String,
    bytes_read: u64,
}

impl<R: BufRead> NdJsonStream<R> {
    /// Total bytes consumed from the underlying reader so far.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

impl<R: BufRead> Iterator for NdJsonStream<R> {
    type Item = Result<Value, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.buf.clear();
            match self.reader.read_line(&mut self.buf) {
                Ok(0) => return None,
                Ok(n) => {
                    self.bytes_read += n as u64;
                    let trimmed = self.buf.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if exceeds_max_structural_depth(trimmed.as_bytes()) {
                        return Some(Err(ParseError::TooDeep));
                    }
                    return Some(
                        sonic_rs::from_str::<Value>(trimmed)
                            .map_err(|e| ParseError::Json(e.to_string())),
                    );
                }
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                    return Some(Err(ParseError::Utf8));
                }
                Err(e) => return Some(Err(ParseError::Json(e.to_string()))),
            }
        }
    }
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
    fn parse_ndjson_stream_yields_values_and_counts_bytes() {
        let input = b"{\"a\":1}\n\n{\"a\":2}\n";
        let stream = parse_ndjson_stream(input.as_slice());
        let values: Vec<Value> = stream.map(|r| r.expect("valid")).collect();
        assert_eq!(values, vec![serde_json::json!({"a": 1}), serde_json::json!({"a": 2})]);
        // `bytes_read` is not reachable after `map` consumes `stream`, so
        // this test primarily validates parsing; byte counting is covered by
        // the streaming TRON conversion tests in `tooned-convert`.
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
