// SPDX-License-Identifier: AGPL-3.0-only

//! YAML parsing.

use serde_json::Value;
use tooned_parse::ParseError;

/// Parses YAML input into a `serde_json::Value`.
pub fn parse_yaml(input: &[u8]) -> Result<Value, ParseError> {
    serde_yaml::from_slice::<Value>(input).map_err(|e| ParseError::Yaml(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_yaml() {
        let value = parse_yaml(b"a: 1\nb:\n  - x\n  - y\n").expect("valid YAML");
        assert_eq!(value, serde_json::json!({"a": 1, "b": ["x", "y"]}));
    }

    #[test]
    fn yaml_brackets_in_string_are_not_false_positive_depth() {
        let yaml = b"a: '[[[[[[[[[[[[[[[[[[[[[[[['";
        assert!(parse_yaml(yaml).is_ok());
    }

    #[test]
    fn yaml_brackets_in_comments_are_not_false_positive_depth() {
        let yaml = b"# [[[[[[[[[[[[[[[[[[[[[[[[\na: 1\n";
        assert!(parse_yaml(yaml).is_ok());
    }
}
