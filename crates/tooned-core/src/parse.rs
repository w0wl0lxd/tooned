//! Parses raw bytes of a detected `DocType` into a canonical
//! `serde_json::Value`.
//!
//! JSON: `sonic_rs::from_slice::<serde_json::Value>` above
//! [`SONIC_RS_THRESHOLD_BYTES`] on x86_64/aarch64 (SIMD-accelerated),
//! `serde_json::from_slice` otherwise. Verified (see
//! `tests/duplicate_keys.rs`, research.md #4) that feeding
//! `sonic_rs::from_slice` straight into `serde_json::Value` -- never through
//! `sonic_rs::Value` -- resolves duplicate object keys identically to plain
//! `serde_json` (last-value-wins).
//!
//! YAML/TOML/CSV/TSV: `serde_yaml`/`toml`/`csv` (BurntSushi), the latter
//! building a `Vec<Map<String, Value>>` -> `Value::Array` per data-model.md.
//!
//! Every branch here returns `Result`, never panics: adversarial/truncated/
//! invalid-UTF-8 input must fold into `ParseError`, which `convert.rs` maps
//! to `PassthroughReason::ParseFailed` (constitution Principle I).

use serde_json::{Map, Value};

use crate::DocType;

/// Threshold (bytes) above which JSON parsing prefers the SIMD-accelerated
/// `sonic-rs` fast path over `serde_json`, on x86_64/aarch64. Chosen as a
/// conservative starting point per research.md #4 ("exact threshold to be
/// tuned during implementation via benchmarking, not fixed at planning
/// time"): below this, `serde_json`'s lower setup overhead tends to win;
/// above it, SIMD parsing has enough bytes to pay for itself. Revisit with
/// `criterion` benchmarks before v1 ships (Polish phase, T077).
pub const SONIC_RS_THRESHOLD_BYTES: usize = 8 * 1024;

/// Conservative nesting-depth guard applied before any of these bytes reach
/// a real deserializer. `serde_json`'s own `Value` deserializer defaults to
/// rejecting recursion past depth ~127; `serde_yaml` and `toml` similarly
/// self-guard well below this. **`sonic-rs`'s deserializer does not** --
/// verified empirically (see module tests below and the T009 no-panic
/// property test): adversarially deep bracket nesting fed through
/// `sonic_rs::from_slice::<serde_json::Value>` overflows the stack rather
/// than returning an `Err` once past roughly depth 150-200 on a 2 MiB
/// thread stack (the default per-test-thread stack size `cargo test`/
/// `cargo nextest` use) -- a fatal, *uncatchable* process abort, not a
/// panic, so it cannot be guarded against after the fact. This scan runs
/// ahead of every parser here (not just the sonic-rs path) so behavior is
/// identical regardless of which deserializer actually handles a given
/// input, and stays well under the 150-200 boundary with a wide safety
/// margin (also protects the *subsequent* recursive operations on a
/// successfully-parsed `Value` -- encode/serialize/Drop -- which have the
/// same recursion-depth-proportional-to-value-nesting shape).
const MAX_STRUCTURAL_DEPTH: usize = 100;

/// Flat, iterative (non-recursive) walk that rejects input nested deeper
/// than [`MAX_STRUCTURAL_DEPTH`] worth of `{`/`[`/`}`/`]`, ignoring bracket
/// characters inside double-quoted strings (with `\"` escape handling).
/// Deliberately shared across JSON/YAML/TOML: all three use this bracket
/// punctuation for nested structures (YAML flow style, TOML inline tables/
/// arrays), and a single-quote-string false positive here (over-counting
/// depth inside a YAML/TOML single-quoted string containing many brackets)
/// only ever makes this *more* conservative, never less safe.
pub(crate) fn exceeds_max_structural_depth(input: &[u8]) -> bool {
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

#[derive(Debug, thiserror::Error)]
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
    #[error("input is not valid UTF-8")]
    Utf8,
    #[error("input nesting exceeds the safe structural-depth limit")]
    TooDeep,
}

pub fn parse(input: &[u8], doc_type: DocType) -> Result<Value, ParseError> {
    match doc_type {
        DocType::Json => parse_json(input),
        DocType::NdJson => parse_ndjson(input),
        DocType::Yaml => parse_yaml(input),
        DocType::Toml => parse_toml(input),
        DocType::Csv => parse_delimited(input, b','),
        DocType::Tsv => parse_delimited(input, b'\t'),
        DocType::Xml => crate::xml::parse(input),
    }
}

fn parse_json(input: &[u8]) -> Result<Value, ParseError> {
    if exceeds_max_structural_depth(input) {
        return Err(ParseError::TooDeep);
    }
    if use_simd_json(input.len()) {
        sonic_rs::from_slice::<Value>(input).map_err(|e| ParseError::Json(e.to_string()))
    } else {
        serde_json::from_slice::<Value>(input).map_err(|e| ParseError::Json(e.to_string()))
    }
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn use_simd_json(len: usize) -> bool {
    len >= SONIC_RS_THRESHOLD_BYTES
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn use_simd_json(_len: usize) -> bool {
    false
}

fn parse_ndjson(input: &[u8]) -> Result<Value, ParseError> {
    let mut items = Vec::new();
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

fn parse_yaml(input: &[u8]) -> Result<Value, ParseError> {
    if exceeds_max_structural_depth(input) {
        return Err(ParseError::TooDeep);
    }
    serde_yaml::from_slice::<Value>(input).map_err(|e| ParseError::Yaml(e.to_string()))
}

fn parse_toml(input: &[u8]) -> Result<Value, ParseError> {
    if exceeds_max_structural_depth(input) {
        return Err(ParseError::TooDeep);
    }
    let text = std::str::from_utf8(input).map_err(|_| ParseError::Utf8)?;
    toml::from_str::<Value>(text).map_err(|e| ParseError::Toml(e.to_string()))
}

/// Builds `Vec<Map<String, Value>>` -> `Value::Array` from a delimited
/// text table, per data-model.md. All fields are kept as JSON strings (CSV
/// has no native types); `flexible(true)` tolerates ragged rows rather than
/// erroring on them -- a ragged CSV is still valid CSV, it just classifies
/// as `Irregular` shape rather than failing to parse at all.
fn parse_delimited(input: &[u8], delimiter: u8) -> Result<Value, ParseError> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_reader(input);

    let headers = reader.headers().map_err(|e| ParseError::Csv(e.to_string()))?.clone();

    // A duplicate column header (e.g. `a,a,b` from a SQL join/export tool)
    // would otherwise silently collapse via `map.insert` below -- the
    // second field overwrites the first under the same key, permanently
    // losing an entire column with no diagnostic. Detected upfront and
    // surfaced as a parse error (which `convert.rs`/`attempt()` maps to a
    // fail-safe passthrough, constitution Principle I) rather than emitting
    // a silently-corrupted `Value`, mirroring how JSON's duplicate-key case
    // is already handled correctly (see `tests/duplicate_keys.rs`).
    if let Some(dup) = first_duplicate_header(&headers) {
        return Err(ParseError::Csv(format!(
            "duplicate column header {dup:?}: refusing to parse, since later columns of the \
             same name would silently overwrite earlier ones and lose data"
        )));
    }

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| ParseError::Csv(e.to_string()))?;
        let mut map = Map::new();
        for (i, field) in record.iter().enumerate() {
            let key = match headers.get(i) {
                Some(k) => k.to_string(),
                None => format!("field_{i}"),
            };
            map.insert(key, Value::String(field.to_string()));
        }
        rows.push(Value::Object(map));
    }
    Ok(Value::Array(rows))
}

/// Returns the first column name that appears more than once in `headers`,
/// or `None` if every header is unique. `O(n)` in the column count via a
/// `HashSet` scan.
fn first_duplicate_header(headers: &csv::StringRecord) -> Option<&str> {
    let mut seen = std::collections::HashSet::with_capacity(headers.len());
    headers.into_iter().find(|candidate| !seen.insert(*candidate))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    #[test]
    fn parses_json_object() {
        let value = parse(br#"{"a": 1, "b": [1, 2]}"#, DocType::Json).expect("valid JSON");
        assert_eq!(value, serde_json::json!({"a": 1, "b": [1, 2]}));
    }

    #[test]
    fn parses_ndjson_into_array() {
        let value = parse(b"{\"a\":1}\n{\"a\":2}\n", DocType::NdJson).expect("valid NDJSON");
        assert_eq!(value, serde_json::json!([{"a": 1}, {"a": 2}]));
    }

    #[test]
    fn parses_yaml() {
        let value = parse(b"a: 1\nb:\n  - x\n  - y\n", DocType::Yaml).expect("valid YAML");
        assert_eq!(value, serde_json::json!({"a": 1, "b": ["x", "y"]}));
    }

    #[test]
    fn parses_toml() {
        let value = parse(b"a = 1\n[b]\nc = \"d\"\n", DocType::Toml).expect("valid TOML");
        assert_eq!(value, serde_json::json!({"a": 1, "b": {"c": "d"}}));
    }

    #[test]
    fn parses_csv_into_array_of_objects() {
        let value = parse(b"name,age\nalice,30\nbob,25\n", DocType::Csv).expect("valid CSV");
        assert_eq!(
            value,
            serde_json::json!([
                {"name": "alice", "age": "30"},
                {"name": "bob", "age": "25"},
            ])
        );
    }

    #[test]
    fn parses_tsv_into_array_of_objects() {
        let value = parse(b"name\tage\nalice\t30\nbob\t25\n", DocType::Tsv).expect("valid TSV");
        assert_eq!(
            value,
            serde_json::json!([
                {"name": "alice", "age": "30"},
                {"name": "bob", "age": "25"},
            ])
        );
    }

    #[test]
    fn ragged_csv_does_not_error() {
        // fewer fields on row 2, extra field on row 3 -- flexible(true) must
        // tolerate this rather than surfacing a parse error.
        let value =
            parse(b"a,b,c\n1,2,3\n4,5\n6,7,8,9\n", DocType::Csv).expect("ragged CSV parses");
        assert!(matches!(value, Value::Array(_)));
    }

    #[test]
    fn duplicate_csv_header_is_an_error_not_silent_data_loss() {
        // Regression test: `a,a,b` (e.g. from a SQL join or export tool)
        // must never silently collapse to `{"a": <second value>, "b": ...}`
        // via `map.insert` overwriting the first `a` column -- that would
        // permanently and undetectably lose data (constitution Principle
        // I). It must surface as a parse error instead, so the caller falls
        // back to passthrough.
        let result = parse(b"a,a,b\n1,2,3\n4,5,6\n", DocType::Csv);
        assert!(
            matches!(result, Err(ParseError::Csv(_))),
            "duplicate CSV header must error, not silently drop a column: {result:?}"
        );
    }

    #[test]
    fn duplicate_tsv_header_is_an_error_not_silent_data_loss() {
        let result = parse(b"a\ta\tb\n1\t2\t3\n4\t5\t6\n", DocType::Tsv);
        assert!(
            matches!(result, Err(ParseError::Csv(_))),
            "duplicate TSV header must error, not silently drop a column: {result:?}"
        );
    }

    #[test]
    fn invalid_json_is_an_error_not_a_panic() {
        assert!(parse(b"{not valid json", DocType::Json).is_err());
    }

    #[test]
    fn invalid_utf8_toml_is_an_error_not_a_panic() {
        assert!(parse(&[0xFF, 0xFE, b'='], DocType::Toml).is_err());
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
        let result = parse(&bytes, DocType::Json);
        assert!(matches!(result, Err(ParseError::TooDeep)));
    }

    #[test]
    fn structural_depth_guard_ignores_brackets_inside_strings() {
        let value = parse(br#"{"a": "[[[[[[[[[[[[[[[[[[[[[[[["}"#, DocType::Json)
            .expect("brackets inside a string must not count toward nesting depth");
        assert_eq!(value, serde_json::json!({"a": "[[[[[[[[[[[[[[[[[[[[[[[["}));
    }

    #[test]
    fn sonic_rs_fast_path_matches_serde_json_for_a_large_payload() {
        let mut s = String::from("[");
        for i in 0..2000 {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(s, r#"{{"id":{i}}}"#);
        }
        s.push(']');
        let bytes = s.into_bytes();
        assert!(bytes.len() >= SONIC_RS_THRESHOLD_BYTES);
        let via_fast_path = parse(&bytes, DocType::Json).expect("valid JSON");
        let via_serde_json: Value = serde_json::from_slice(&bytes).expect("valid JSON");
        assert_eq!(via_fast_path, via_serde_json);
    }
}
