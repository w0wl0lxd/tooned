// SPDX-License-Identifier: AGPL-3.0-only

//! MessagePack parsing.

use serde_json::Value;
use tooned_parse::ParseError;

/// Parses a MessagePack byte slice into a `serde_json::Value`.
///
/// Non-string map keys are rejected by `serde_json::Value`, which is the
/// desired behaviour: JSON-shaped tool-call data is expected to use
/// string keys, and silently coercing integer keys to strings would change
/// the data model.
pub fn parse_msgpack(input: &[u8]) -> Result<Value, ParseError> {
    rmp_serde::from_slice::<Value>(input).map_err(|e| ParseError::Msgpack(e.to_string()))
}
