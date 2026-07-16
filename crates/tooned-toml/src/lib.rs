// SPDX-License-Identifier: AGPL-3.0-only

//! TOML parsing.

use serde_json::Value;
use tooned_parse::ParseError;

/// Parses TOML input into a `serde_json::Value`.
pub fn parse_toml(input: &[u8]) -> Result<Value, ParseError> {
    let text = std::str::from_utf8(input).map_err(|_| ParseError::Utf8)?;
    toml::from_str::<Value>(text).map_err(|e| ParseError::Toml(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_toml() {
        let value = parse_toml(b"a = 1\n[b]\nc = \"d\"\n").expect("valid TOML");
        assert_eq!(value, serde_json::json!({"a": 1, "b": {"c": "d"}}));
    }

    #[test]
    fn toml_brackets_in_string_are_not_false_positive_depth() {
        let toml = b"a = \"[[[[[[[[[[[[[[[[[[[[[[[[\"";
        assert!(parse_toml(toml).is_ok());
    }

    #[test]
    fn invalid_utf8_toml_is_an_error_not_a_panic() {
        assert!(parse_toml(&[0xFF, 0xFE, b'=']).is_err());
    }
}
