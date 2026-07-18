// SPDX-License-Identifier: AGPL-3.0-only

//! CSV/TSV parsing.

use std::io::BufRead;

use serde_json::{Map, Value};
use tooned_parse::ParseError;

/// Parses CSV input into a `serde_json::Value` (as an array of objects).
pub fn parse_csv(input: &[u8]) -> Result<Value, ParseError> {
    parse_delimited(input, b',')
}

/// Parses TSV input into a `serde_json::Value` (as an array of objects).
pub fn parse_tsv(input: &[u8]) -> Result<Value, ParseError> {
    parse_delimited(input, b'\t')
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

    let headers = reader.headers().map_err(|e| ParseError::Csv(e.to_string()))?;

    // A duplicate column header (e.g. `a,a,b` from a SQL join/export tool)
    // would otherwise silently collapse via `map.insert` below -- the
    // second field overwrites the first under the same key, permanently
    // losing an entire column with no diagnostic. Detected upfront and
    // surfaced as a parse error (which `convert.rs`/`attempt()` maps to a
    // fail-safe passthrough, constitution Principle I) rather than emitting
    // a silently-corrupted `Value`, mirroring how JSON's duplicate-key case
    // is already handled correctly (see `tests/duplicate_keys.rs`).
    if let Some(dup) = first_duplicate_header(headers) {
        return Err(ParseError::Csv(format!(
            "duplicate column header {dup:?}: refusing to parse, since later columns of the \
             same name would silently overwrite earlier ones and lose data"
        )));
    }

    let headers = headers.clone();

    #[allow(clippy::naive_bytecount)]
    let estimated_rows = input.iter().filter(|&&b| b == b'\n').count() + 1;
    let mut rows = Vec::with_capacity(estimated_rows);
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

/// Streaming CSV parser: yields one `Value` per record as an object.
pub fn parse_csv_stream<R: BufRead>(reader: R) -> CsvStream<R> {
    let mut csv_reader = csv::ReaderBuilder::new()
        .delimiter(b',')
        .has_headers(true)
        .flexible(true)
        .from_reader(reader);

    // Read headers immediately
    let headers_result = csv_reader.headers();
    let headers = match headers_result {
        Ok(h) => {
            if let Some(dup) = first_duplicate_header(&h) {
                let dup = dup.to_string();
                Err(format!(
                    "duplicate column header {dup:?}: refusing to parse, since later columns of the \
                     same name would silently overwrite earlier ones and lose data"
                ))
            } else {
                Ok(h.iter().map(|s| s.to_string()).collect())
            }
        }
        Err(e) => Err(e.to_string()),
    };

    CsvStream { reader: csv_reader, headers }
}

/// Iterator returned by [`parse_csv_stream`].
pub struct CsvStream<R> {
    reader: csv::Reader<R>,
    headers: Result<Vec<String>, String>,
}

impl<R: BufRead> CsvStream<R> {
    /// Get the headers, if available.
    pub fn headers(&self) -> Option<&[String]> {
        self.headers.as_ref().ok().map(|h| h.as_slice())
    }
}

impl<R: BufRead> Iterator for CsvStream<R> {
    type Item = Result<Value, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Check for header error first
        let headers = match &self.headers {
            Ok(h) => h,
            Err(e) => return Some(Err(ParseError::Csv(e.clone()))),
        };

        let record = match self.reader.records().next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Some(Err(ParseError::Csv(e.to_string()))),
            None => return None,
        };

        let mut map = Map::new();
        for (i, field) in record.iter().enumerate() {
            let key = match headers.get(i) {
                Some(k) => k.clone(),
                None => format!("field_{i}"),
            };
            map.insert(key, Value::String(field.to_string()));
        }
        Some(Ok(Value::Object(map)))
    }
}

/// Streaming TSV parser: yields one `Value` per record as an object.
pub fn parse_tsv_stream<R: BufRead>(reader: R) -> TsvStream<R> {
    let mut csv_reader = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .flexible(true)
        .from_reader(reader);

    // Read headers immediately
    let headers_result = csv_reader.headers();
    let headers = match headers_result {
        Ok(h) => {
            if let Some(dup) = first_duplicate_header(&h) {
                let dup = dup.to_string();
                Err(format!(
                    "duplicate column header {dup:?}: refusing to parse, since later columns of the \
                     same name would silently overwrite earlier ones and lose data"
                ))
            } else {
                Ok(h.iter().map(|s| s.to_string()).collect())
            }
        }
        Err(e) => Err(e.to_string()),
    };

    TsvStream { reader: csv_reader, headers }
}

/// Iterator returned by [`parse_tsv_stream`].
pub struct TsvStream<R> {
    reader: csv::Reader<R>,
    headers: Result<Vec<String>, String>,
}

impl<R: BufRead> TsvStream<R> {
    /// Get the headers, if available.
    pub fn headers(&self) -> Option<&[String]> {
        self.headers.as_ref().ok().map(|h| h.as_slice())
    }
}

impl<R: BufRead> Iterator for TsvStream<R> {
    type Item = Result<Value, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Check for header error first
        let headers = match &self.headers {
            Ok(h) => h,
            Err(e) => return Some(Err(ParseError::Csv(e.clone()))),
        };

        let record = match self.reader.records().next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Some(Err(ParseError::Csv(e.to_string()))),
            None => return None,
        };

        let mut map = Map::new();
        for (i, field) in record.iter().enumerate() {
            let key = match headers.get(i) {
                Some(k) => k.clone(),
                None => format!("field_{i}"),
            };
            map.insert(key, Value::String(field.to_string()));
        }
        Some(Ok(Value::Object(map)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_csv_into_array_of_objects() {
        let value = parse_csv(b"name,age\nalice,30\nbob,25\n").expect("valid CSV");
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
        let value = parse_tsv(b"name\tage\nalice\t30\nbob\t25\n").expect("valid TSV");
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
        let value = parse_csv(b"a,b,c\n1,2,3\n4,5\n6,7,8,9\n").expect("ragged CSV parses");
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
        let result = parse_csv(b"a,a,b\n1,2,3\n4,5,6\n");
        assert!(
            matches!(result, Err(ParseError::Csv(_))),
            "duplicate CSV header must error, not silently drop a column: {result:?}"
        );
    }

    #[test]
    fn duplicate_tsv_header_is_an_error_not_silent_data_loss() {
        let result = parse_tsv(b"a\ta\tb\n1\t2\t3\n4\t5\t6\n");
        assert!(
            matches!(result, Err(ParseError::Csv(_))),
            "duplicate TSV header must error, not silently drop a column: {result:?}"
        );
    }
}
