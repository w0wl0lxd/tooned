// SPDX-License-Identifier: AGPL-3.0-only

//! JSON5 parsing.

use serde_json::Value;
use tooned_parse::ParseError;

/// Parses a JSON5 text slice into a `serde_json::Value`.
///
/// JSON5 is a superset of JSON that allows comments, trailing commas,
/// unquoted object keys, and single-quoted strings. This makes it useful
/// for hand-written configuration files that `tooned` may encounter as
/// tool-call input.
pub fn parse_json5(input: &[u8]) -> Result<Value, ParseError> {
    let text = std::str::from_utf8(input).map_err(|_| ParseError::Utf8)?;
    json5::from_str::<Value>(text).map_err(|e| ParseError::Json5(e.to_string()))
}
