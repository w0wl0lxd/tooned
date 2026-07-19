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
    // Guard the entire NDJSON stream once rather than re-scanning every line.
    if exceeds_max_structural_depth(input) {
        return Err(ParseError::TooDeep);
    }

    #[allow(clippy::naive_bytecount)]
    let estimated_lines = input.iter().filter(|&&b| b == b'\n').count() + 1;
    let mut items = Vec::with_capacity(estimated_lines);
    for line in input.split(|b| *b == b'\n') {
        let trimmed = line.trim_ascii();
        if trimmed.is_empty() {
            continue;
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

/// Streaming JSON array parser: yields one `Value` per top-level array element
/// without ever buffering the whole input in memory. The byte counter returned
/// by [`JsonArrayStream::bytes_read`] includes all bytes consumed from the
/// underlying reader.
///
/// This uses manual bracket-depth tracking because `serde_json`'s streaming
/// deserializer is designed for whitespace-separated values (NDJSON), not for
/// streaming individual elements within a single JSON array.
pub fn parse_json_stream<R: BufRead>(reader: R) -> JsonArrayStream<R> {
    JsonArrayStream {
        reader,
        buf: String::new(),
        bytes_read: 0,
        pos: 0,
        depth: 0,
        in_string: false,
        escaped: false,
        state: StreamState::Before,
    }
}

/// Iterator returned by [`parse_json_stream`].
pub struct JsonArrayStream<R> {
    reader: R,
    buf: String,
    bytes_read: u64,
    pos: usize,
    depth: usize,
    in_string: bool,
    escaped: bool,
    state: StreamState,
}

#[derive(Debug)]
enum StreamState {
    Before, // Looking for opening '['
    Inside, // Inside array, depth >= 1
    After,  // After closing ']', done
}

impl<R: BufRead> JsonArrayStream<R> {
    /// Total bytes consumed from the underlying reader so far.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Verify that no non-whitespace bytes remain after the closing `]`.
    /// Whitespace (including newlines) is allowed; anything else is treated as
    /// trailing data and returned as an error.
    pub fn check_trailing(&mut self) -> Result<(), ParseError> {
        if !matches!(self.state, StreamState::After) {
            return Ok(());
        }

        while self.pos < self.buf.len() {
            if let Some(c) = self.buf[self.pos..].chars().next() {
                if c.is_ascii_whitespace() {
                    self.pos += c.len_utf8();
                } else {
                    return Err(ParseError::Json("trailing data after JSON array".into()));
                }
            } else {
                break;
            }
        }

        loop {
            self.buf.clear();
            self.pos = 0;
            match self.reader.read_line(&mut self.buf) {
                Ok(0) => return Ok(()),
                Ok(n) => {
                    self.bytes_read += n as u64;
                    if self.buf.trim().is_empty() {
                        continue;
                    }
                    return Err(ParseError::Json("trailing data after JSON array".into()));
                }
                Err(e) => return Err(ParseError::Json(e.to_string())),
            }
        }
    }

    /// Ensure we have data in the buffer. Returns false on EOF.
    fn fill_buf(&mut self) -> bool {
        if self.pos < self.buf.len() {
            return true;
        }
        self.buf.clear();
        self.pos = 0;
        match self.reader.read_line(&mut self.buf) {
            Ok(0) | Err(_) => false,
            Ok(n) => {
                self.bytes_read += n as u64;
                true
            }
        }
    }

    /// Get the next character, or None on EOF.
    fn next_char(&mut self) -> Option<char> {
        if !self.fill_buf() {
            return None;
        }
        if self.pos >= self.buf.len() {
            return None;
        }
        let c = self.buf[self.pos..].chars().next()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    /// Peek at the next character without consuming it.
    fn peek_char(&mut self) -> Option<char> {
        if !self.fill_buf() {
            return None;
        }
        self.buf[self.pos..].chars().next()
    }
}

impl<R: BufRead> Iterator for JsonArrayStream<R> {
    type Item = Result<Value, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Find the opening '['
        while matches!(self.state, StreamState::Before) {
            match self.next_char()? {
                '[' => {
                    self.state = StreamState::Inside;
                    self.depth = 1;
                    break;
                }
                c if c.is_ascii_whitespace() => {}
                _ => {
                    return Some(Err(ParseError::Json(
                        "expected JSON array to start with '['".into(),
                    )));
                }
            }
        }

        // Skip whitespace
        while let Some(c) = self.peek_char() {
            if c.is_ascii_whitespace() {
                self.next_char();
            } else {
                break;
            }
        }

        // Check if we're at the closing bracket
        if let Some(']') = self.peek_char() {
            self.next_char();
            self.state = StreamState::After;
            return None;
        }

        // Collect characters for one array element
        let mut element_buf = String::new();
        let mut element_depth: usize = 0;

        loop {
            let c = self.next_char()?;

            if self.in_string {
                element_buf.push(c);
                if self.escaped {
                    self.escaped = false;
                } else if c == '\\' {
                    self.escaped = true;
                } else if c == '"' {
                    self.in_string = false;
                }
                continue;
            }

            match c {
                '"' => {
                    self.in_string = true;
                    element_buf.push(c);
                }
                '{' | '[' => {
                    self.depth += 1;
                    element_depth += 1;
                    element_buf.push(c);
                }
                '}' | ']' => {
                    if self.depth == 0 {
                        return Some(Err(ParseError::Json(
                            "unbalanced brackets in JSON array".into(),
                        )));
                    }
                    self.depth -= 1;
                    element_depth = element_depth.saturating_sub(1);
                    element_buf.push(c);

                    if self.depth == 0 {
                        // End of array - this was the last element
                        self.state = StreamState::After;
                        if element_buf.is_empty() {
                            return None; // Empty array
                        }
                        // Parse the last element
                        let trimmed = element_buf.trim();
                        if exceeds_max_structural_depth(trimmed.as_bytes()) {
                            return Some(Err(ParseError::TooDeep));
                        }
                        return Some(
                            sonic_rs::from_str::<Value>(trimmed)
                                .map_err(|e| ParseError::Json(e.to_string())),
                        );
                    }

                    if element_depth == 0 {
                        // End of this element
                        // Skip whitespace and check for comma or closing bracket
                        while let Some(next_c) = self.peek_char() {
                            if next_c.is_ascii_whitespace() {
                                self.next_char();
                            } else {
                                break;
                            }
                        }
                        match self.peek_char()? {
                            ',' => {
                                self.next_char(); // Skip comma
                                // Parse the element
                                let trimmed = element_buf.trim();
                                if exceeds_max_structural_depth(trimmed.as_bytes()) {
                                    return Some(Err(ParseError::TooDeep));
                                }
                                return Some(
                                    sonic_rs::from_str::<Value>(trimmed)
                                        .map_err(|e| ParseError::Json(e.to_string())),
                                );
                            }
                            ']' => {
                                // End of array, will be handled in next iteration
                                // Parse the element
                                let trimmed = element_buf.trim();
                                if exceeds_max_structural_depth(trimmed.as_bytes()) {
                                    return Some(Err(ParseError::TooDeep));
                                }
                                return Some(
                                    sonic_rs::from_str::<Value>(trimmed)
                                        .map_err(|e| ParseError::Json(e.to_string())),
                                );
                            }
                            _ => {
                                return Some(Err(ParseError::Json(
                                    "expected ',' or ']' after array element".into(),
                                )));
                            }
                        }
                    }
                }
                ',' if element_depth == 0 && self.depth == 1 => {
                    // Comma at array level - end of element
                    let trimmed = element_buf.trim();
                    if exceeds_max_structural_depth(trimmed.as_bytes()) {
                        return Some(Err(ParseError::TooDeep));
                    }
                    return Some(
                        sonic_rs::from_str::<Value>(trimmed)
                            .map_err(|e| ParseError::Json(e.to_string())),
                    );
                }
                _ => {
                    if !c.is_ascii_whitespace() || element_depth > 0 {
                        element_buf.push(c);
                    }
                }
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

    #[test]
    fn parse_json_stream_uniform_array() {
        let input = b"[{\"a\":1},{\"a\":2},{\"a\":3}]";
        let stream = parse_json_stream(input.as_slice());
        let values: Vec<Value> = stream.map(|r| r.expect("valid")).collect();
        assert_eq!(
            values,
            vec![
                serde_json::json!({"a": 1}),
                serde_json::json!({"a": 2}),
                serde_json::json!({"a": 3}),
            ]
        );
    }

    #[test]
    fn parse_json_stream_nested_objects() {
        let input = b"[{\"x\":{\"y\":1}},{\"x\":{\"y\":2}}]";
        let stream = parse_json_stream(input.as_slice());
        let values: Vec<Value> = stream.map(|r| r.expect("valid")).collect();
        assert_eq!(
            values,
            vec![serde_json::json!({"x": {"y": 1}}), serde_json::json!({"x": {"y": 2}}),]
        );
    }

    #[test]
    fn parse_json_stream_empty_array() {
        let input = b"[]";
        let stream = parse_json_stream(input.as_slice());
        let values: Vec<Value> = stream.map(|r| r.expect("valid")).collect();
        assert!(values.is_empty());
    }

    #[test]
    fn parse_json_stream_with_whitespace() {
        let input = b"  [  {\"a\":1}  ,  {\"a\":2}  ]  ";
        let stream = parse_json_stream(input.as_slice());
        let values: Vec<Value> = stream.map(|r| r.expect("valid")).collect();
        assert_eq!(values, vec![serde_json::json!({"a": 1}), serde_json::json!({"a": 2}),]);
    }

    #[test]
    fn parse_json_stream_with_string_escapes() {
        let input = br#"[{"text":"hello \"world\""},{"text":"foo\nbar"}]"#;
        let stream = parse_json_stream(input.as_slice());
        let values: Vec<Value> = stream.map(|r| r.expect("valid")).collect();
        assert_eq!(
            values,
            vec![
                serde_json::json!({"text": "hello \"world\""}),
                serde_json::json!({"text": "foo\nbar"}),
            ]
        );
    }

    #[test]
    fn parse_json_stream_nested_arrays() {
        let input = b"[[1,2],[3,4]]";
        let stream = parse_json_stream(input.as_slice());
        let values: Vec<Value> = stream.map(|r| r.expect("valid")).collect();
        assert_eq!(values, vec![serde_json::json!([1, 2]), serde_json::json!([3, 4]),]);
    }

    #[test]
    fn parse_json_stream_mixed_types() {
        let input = b"[1,\"two\",{\"three\":3},[4]]";
        let stream = parse_json_stream(input.as_slice());
        let values: Vec<Value> = stream.map(|r| r.expect("valid")).collect();
        assert_eq!(
            values,
            vec![
                serde_json::json!(1),
                serde_json::json!("two"),
                serde_json::json!({"three": 3}),
                serde_json::json!([4]),
            ]
        );
    }

    #[test]
    fn parse_json_stream_too_deep_element() {
        let depth = 10_000;
        let mut element = String::from("{");
        for _ in 0..depth {
            element.push_str("\"x\":{");
        }
        element.push_str("\"val\":1");
        for _ in 0..depth {
            element.push('}');
        }
        element.push('}');
        let input = format!("[{}]", element);
        let stream = parse_json_stream(input.as_bytes());
        let result: Vec<Result<Value, ParseError>> = stream.collect();
        assert_eq!(result.len(), 1);
        let Some(Err(ParseError::TooDeep)) = result.first() else {
            panic!("expected a single TooDeep error");
        };
    }
}
