// SPDX-License-Identifier: AGPL-3.0-only

//! CBOR parsing.

use serde_json::Value;
use tooned_parse::ParseError;

/// Parses a CBOR byte slice into a `serde_json::Value`.
///
/// `cbor4ii` is used because it is small, pure-Rust, and supports the
/// `serde` data model without pulling in the `serde_cbor` maintenance
/// liability. Non-string map keys are rejected by `serde_json::Value`,
/// preserving JSON fidelity.
pub fn parse_cbor(input: &[u8]) -> Result<Value, ParseError> {
    cbor4ii::serde::from_slice::<Value>(input).map_err(|e| ParseError::Cbor(e.to_string()))
}
