// SPDX-License-Identifier: AGPL-3.0-only

//! Prototype TRON (Token-Reduced Object Notation) record-stream encoder/decoder.
//!
//! TRON hoists repeated object schemas into a class-definition header, then
//! emits each record as a compact `ClassName(value, value, ...)` call. The body
//! is a JSON superset: class calls may appear anywhere a JSON value is valid,
//! including inside arrays and nested objects.
//!
//! This prototype supports a single top-level class (`A`) derived from the
//! first object in a uniform array, or from a single top-level object. Nested
//! values must be primitive for the encoder, but the decoder expands any
//! class calls found in the body.

use std::collections::HashMap;
use std::io::{BufRead, Write};

use serde_json::{Map, Value};
use tooned_types::{
    Conversion, ConversionOptions, ConversionReport, PassthroughReason, ToonedError,
};

use crate::shape;

const CLASS_PREFIX: &str = "class ";

/// Encode a uniform array of flat objects (or a single flat object) as TRON text.
///
/// The encoder produces a class header followed by a blank line and a body
/// of JSON-with-class-calls. All objects must share the same key set in the
/// same order, and every value must be primitive (`null`, boolean, number,
/// or string). Keys are emitted as-is in the header, so they must not contain
/// commas, colons, or whitespace.
pub fn encode(value: &Value) -> Result<String, ToonedError> {
    let rows = rows_for(value)?;
    let keys = rows.0;
    let values = rows.1;

    let mut out = class_header("A", &keys);

    if let Value::Array(_) = value {
        out.push('[');
        for (i, row) in values.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&encode_instance(row));
        }
        out.push(']');
    } else {
        // Single top-level object.
        let row = values
            .first()
            .ok_or_else(|| ToonedError::DecodeFailed("TRON missing single object row".into()))?;
        out.push_str(&encode_instance(row));
    }

    Ok(out)
}

fn rows_for(value: &Value) -> Result<(Vec<String>, Vec<Vec<String>>), ToonedError> {
    match value {
        Value::Object(obj) => {
            let (keys, vals) = object_row(obj, 0)?;
            Ok((keys, vec![vals]))
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                return Err(ToonedError::DecodeFailed("TRON cannot encode an empty array".into()));
            }
            let first = arr.first().ok_or_else(|| {
                ToonedError::DecodeFailed("TRON array is unexpectedly empty".into())
            })?;
            let (keys, first_vals) = object_row(
                first.as_object().ok_or_else(|| {
                    ToonedError::DecodeFailed("TRON array elements must be objects".into())
                })?,
                0,
            )?;
            let mut rows = Vec::with_capacity(arr.len());
            rows.push(first_vals);
            for (i, item) in arr.iter().enumerate().skip(1) {
                let (item_keys, item_vals) = object_row(
                    item.as_object().ok_or_else(|| {
                        ToonedError::DecodeFailed(format!(
                            "TRON array element {i} is not an object"
                        ))
                    })?,
                    i,
                )?;
                if item_keys != keys {
                    return Err(ToonedError::DecodeFailed(format!(
                        "TRON array element {i} has a different key set or order than the first element"
                    )));
                }
                rows.push(item_vals);
            }
            Ok((keys, rows))
        }
        _ => Err(ToonedError::DecodeFailed(
            "TRON requires a top-level object or array of objects".into(),
        )),
    }
}

fn object_keys(obj: &Map<String, Value>, index: usize) -> Result<Vec<String>, ToonedError> {
    if obj.is_empty() {
        return Err(ToonedError::DecodeFailed(format!("TRON object {index} has no keys")));
    }

    let mut keys = Vec::with_capacity(obj.len());
    for key in obj.keys() {
        if !is_valid_header_key(key) {
            return Err(ToonedError::DecodeFailed(format!(
                "TRON object key {key:?} is not a valid header identifier"
            )));
        }
        keys.push(key.clone());
    }
    Ok(keys)
}

fn object_values(
    obj: &Map<String, Value>,
    keys: &[String],
    index: usize,
) -> Result<Vec<String>, ToonedError> {
    if obj.len() != keys.len() {
        return Err(ToonedError::DecodeFailed(format!(
            "TRON object {index} has a different key set than the first element"
        )));
    }

    let mut vals = Vec::with_capacity(keys.len());
    for key in keys {
        let value = obj.get(key).ok_or_else(|| {
            ToonedError::DecodeFailed(format!("TRON object {index} missing key {key}"))
        })?;
        if !is_primitive(value) {
            return Err(ToonedError::DecodeFailed(format!(
                "TRON object {index}, key {key} contains a non-primitive value"
            )));
        }
        vals.push(sonic_rs::to_string(value).map_err(|e| {
            ToonedError::DecodeFailed(format!("failed to serialize TRON cell: {e}"))
        })?);
    }
    Ok(vals)
}

fn object_row(
    obj: &Map<String, Value>,
    index: usize,
) -> Result<(Vec<String>, Vec<String>), ToonedError> {
    let keys = object_keys(obj, index)?;
    let vals = object_values(obj, &keys, index)?;
    Ok((keys, vals))
}

fn class_header(class_name: &str, keys: &[String]) -> String {
    let mut out = String::with_capacity(CLASS_PREFIX.len() + class_name.len() + 2 + keys.len() * 8);
    out.push_str(CLASS_PREFIX);
    out.push_str(class_name);
    out.push(':');
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push(' ');
        out.push_str(key);
    }
    out.push('\n');
    out.push('\n');
    out
}

fn encode_instance(values: &[String]) -> String {
    let mut out =
        String::with_capacity(values.iter().map(String::len).sum::<usize>() + values.len() + 3);
    out.push('A');
    out.push('(');
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(v);
    }
    out.push(')');
    out
}

fn is_primitive(value: &Value) -> bool {
    !matches!(value, Value::Object(_) | Value::Array(_))
}

fn is_valid_header_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Decode TRON text back to a `serde_json::Value`.
///
/// If the text has no class header, it is parsed as plain JSON. Otherwise the
/// header is parsed into a class map and every `ClassName(...)` call in the
/// body is expanded into a JSON object before the body is parsed.
pub fn decode(text: &str) -> Result<Value, ToonedError> {
    let (classes, body) = parse_header(text)?;
    if classes.is_empty() {
        sonic_rs::from_str(body.trim())
            .map_err(|e| ToonedError::DecodeFailed(format!("TRON body is not valid JSON: {e}")))
    } else {
        let json_text = expand_tron(body, &classes)?;
        sonic_rs::from_str(&json_text).map_err(|e| {
            ToonedError::DecodeFailed(format!("expanded TRON body is not valid JSON: {e}"))
        })
    }
}

fn parse_header(text: &str) -> Result<(HashMap<String, Vec<String>>, &str), ToonedError> {
    let mut classes = HashMap::new();
    let mut offset = 0;
    let mut in_header = false;

    for line in text.split_inclusive('\n') {
        let content = match line.strip_suffix('\n') {
            Some(s) => s,
            None => line,
        };
        let content = match content.strip_suffix('\r') {
            Some(s) => s,
            None => content,
        };
        let trimmed = content.trim();

        if trimmed.is_empty() {
            if in_header {
                // separator: body starts after this line
                offset += line.len();
                break;
            }
            // still leading whitespace
            offset += line.len();
            continue;
        }

        if !in_header {
            if trimmed.starts_with(CLASS_PREFIX) {
                in_header = true;
            } else {
                return Ok((classes, text));
            }
        }

        if !trimmed.starts_with(CLASS_PREFIX) {
            return Err(ToonedError::DecodeFailed(format!(
                "expected TRON class definition, found: {trimmed}"
            )));
        }

        let rest = &trimmed[CLASS_PREFIX.len()..];
        let (name, fields) = rest.split_once(':').ok_or_else(|| {
            ToonedError::DecodeFailed(format!("invalid TRON class definition: {trimmed}"))
        })?;
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(ToonedError::DecodeFailed("TRON class name is empty".into()));
        }
        let fields: Vec<String> =
            fields.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        if fields.is_empty() {
            return Err(ToonedError::DecodeFailed(format!("TRON class {name} has no fields")));
        }
        classes.insert(name, fields);
        offset += line.len();
    }

    let body = &text[offset..];
    Ok((classes, body))
}

#[allow(clippy::indexing_slicing)]
fn expand_tron(s: &str, classes: &HashMap<String, Vec<String>>) -> Result<String, ToonedError> {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut out = String::with_capacity(s.len());
    let mut idx = 0;
    let mut in_string = false;
    let mut escape = false;

    while idx < chars.len() {
        let (pos, c) = chars[idx];

        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            idx += 1;
            continue;
        }

        if c == '"' {
            out.push(c);
            in_string = true;
            idx += 1;
            continue;
        }

        if c.is_ascii_uppercase() {
            let name_start_byte = pos;
            while idx < chars.len() && chars[idx].1.is_ascii_uppercase() {
                idx += 1;
            }
            let name_end_byte = if idx < chars.len() { chars[idx].0 } else { s.len() };
            let name = &s[name_start_byte..name_end_byte];

            if idx < chars.len() && chars[idx].1 == '(' {
                let open_idx = idx;
                let close_idx = find_closing_paren(&chars, open_idx)?;
                let args_start_byte = chars[open_idx].0 + 1; // after '('
                let args_end_byte = chars[close_idx].0; // before ')'
                let args_str = &s[args_start_byte..args_end_byte];
                let args = split_args(args_str)?;
                let mut expanded = Vec::with_capacity(args.len());
                for arg in args {
                    expanded.push(expand_tron(arg.trim(), classes)?);
                }
                let fields = classes.get(name).ok_or_else(|| {
                    ToonedError::DecodeFailed(format!("undefined TRON class: {name}"))
                })?;
                if fields.len() != expanded.len() {
                    return Err(ToonedError::DecodeFailed(format!(
                        "TRON class {name} expects {} arguments, got {}",
                        fields.len(),
                        expanded.len()
                    )));
                }
                out.push('{');
                for (i, (field, arg)) in fields.iter().zip(expanded.iter()).enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push('"');
                    // Header keys are already validated to be simple
                    // identifiers, so they need no JSON escaping.
                    out.push_str(field);
                    out.push('"');
                    out.push(':');
                    out.push_str(arg);
                }
                out.push('}');
                idx = close_idx + 1;
                continue;
            }

            // Not a class call: just emit the uppercase run.
            out.push_str(name);
            continue;
        }

        out.push(c);
        idx += 1;
    }

    Ok(out)
}

#[allow(clippy::indexing_slicing)]
fn find_closing_paren(chars: &[(usize, char)], open_idx: usize) -> Result<usize, ToonedError> {
    let mut depth: i32 = 1;
    let mut sub_in_string = false;
    let mut sub_escape = false;
    let mut idx = open_idx + 1;

    while idx < chars.len() {
        let (_, c) = chars[idx];
        if sub_in_string {
            if sub_escape {
                sub_escape = false;
            } else if c == '\\' {
                sub_escape = true;
            } else if c == '"' {
                sub_in_string = false;
            }
        } else if c == '"' {
            sub_in_string = true;
        } else if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                return Ok(idx);
            }
        }
        idx += 1;
    }

    Err(ToonedError::DecodeFailed("unclosed TRON class call".into()))
}

#[allow(clippy::indexing_slicing)]
fn split_args(s: &str) -> Result<Vec<&str>, ToonedError> {
    if s.trim().is_empty() {
        return Ok(Vec::new());
    }

    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut args = Vec::new();
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut start_byte = 0;

    for (pos, c) in &chars {
        if in_string {
            if escape {
                escape = false;
            } else if *c == '\\' {
                escape = true;
            } else if *c == '"' {
                in_string = false;
            }
            continue;
        }

        if *c == '"' {
            in_string = true;
            continue;
        }

        match *c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return Err(ToonedError::DecodeFailed(
                        "unbalanced delimiters in TRON class arguments".into(),
                    ));
                }
            }
            ',' if depth == 0 => {
                args.push(&s[start_byte..*pos]);
                start_byte = pos + c.len_utf8();
            }
            _ => {}
        }
    }

    args.push(&s[start_byte..s.len()]);
    Ok(args)
}

/// Byte-counting writer used to measure compact JSON length without an owned
/// `String`.
struct ByteCountingWriter(usize);

impl std::io::Write for ByteCountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Run the full detect/parse/encode/margin/round-trip pipeline for TRON.
///
/// Mirrors `maybe_tooned` semantics: payload-driven failures downgrade to
/// `Conversion::Passthrough`, never `Err`.
pub fn maybe_tron(input: &[u8], opts: &ConversionOptions) -> Result<Conversion, ToonedError> {
    if input.len() > opts.max_input_bytes {
        return Ok(Conversion::Passthrough {
            bytes: input.to_vec(),
            reason: PassthroughReason::InputTooLarge,
        });
    }

    let Some(doc_type) = tooned_detect::detect(input, opts.format_hint) else {
        return Ok(Conversion::Passthrough {
            bytes: input.to_vec(),
            reason: PassthroughReason::NotStructuredData,
        });
    };

    let Ok(value) = crate::parse_by_doc_type(input, doc_type) else {
        return Ok(Conversion::Passthrough {
            bytes: input.to_vec(),
            reason: PassthroughReason::ParseFailed,
        });
    };

    let shape = shape::classify(&value);

    let mut counter = ByteCountingWriter(0);
    {
        let mut writer = sonic_rs::writer::BufferedWriter::new(&mut counter);
        sonic_rs::to_writer(&mut writer, &value).map_err(|e| {
            ToonedError::DecodeFailed(format!(
                "failed to compute JSON size for TRON comparison: {e}"
            ))
        })?;
        writer.flush().map_err(|e| {
            ToonedError::DecodeFailed(format!("failed to flush TRON JSON byte counter: {e}"))
        })?;
    }
    let json_bytes = counter.0;

    let Ok(encoded) = encode(&value) else {
        return Ok(Conversion::Passthrough {
            bytes: input.to_vec(),
            reason: PassthroughReason::ParseFailed,
        });
    };
    let tron_bytes = encoded.len();

    if !crate::is_smaller_enough(json_bytes, tron_bytes, opts.margin_pct) {
        return Ok(Conversion::Passthrough {
            bytes: input.to_vec(),
            reason: PassthroughReason::NotSmallerEnough { json_bytes, toon_bytes: tron_bytes },
        });
    }

    let round_trip_ok = match decode(&encoded) {
        Ok(decoded) => decoded == value,
        Err(_) => false,
    };

    if !round_trip_ok {
        return Ok(Conversion::Passthrough {
            bytes: input.to_vec(),
            reason: PassthroughReason::RoundTripMismatch,
        });
    }

    Ok(Conversion::Toon {
        text: encoded,
        report: ConversionReport {
            doc_type,
            shape,
            json_bytes,
            toon_bytes: tron_bytes,
            savings_pct: crate::compute_savings_pct(json_bytes, tron_bytes),
        },
    })
}

/// Byte and I/O counters for the streaming TRON encoder.
#[derive(Debug)]
pub struct StreamStats {
    /// Bytes consumed from the input reader (including line delimiters).
    pub input_bytes: u64,
    /// Bytes written to the output writer.
    pub output_bytes: u64,
}

/// Wraps a [`Write`] sink and counts bytes without materializing the full
/// output in memory.
struct CountingWriter<W> {
    inner: W,
    count: u64,
}

impl<W> CountingWriter<W> {
    fn new(inner: W) -> Self {
        Self { inner, count: 0 }
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.count += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Stream-convert an NDJSON/JSONL reader into a TRON record-stream writer.
///
/// The first flat object establishes the class schema; subsequent records that
/// match it are emitted as `A(...)` instances. Records that do not match
/// (different keys, non-primitive values, scalars, arrays) are emitted as
/// ordinary JSON values inside the same top-level array, so the output is
/// always a valid TRON body and no data is lost. Empty input yields `[]`.
///
/// Returns counts of input and output bytes so callers can apply the usual
/// adaptive size gate. Parse errors are propagated as [`ToonedError`] so the
/// caller can fall back to a verbatim passthrough.
pub fn maybe_tron_stream<R, W>(reader: R, writer: &mut W) -> Result<StreamStats, ToonedError>
where
    R: BufRead,
    W: Write,
{
    let mut stream = tooned_json::parse_ndjson_stream(reader);
    let mut out = CountingWriter::new(writer);
    let mut first = true;
    let mut keys: Option<Vec<String>> = None;
    let mut header = String::new();
    let mut array_opened = false;

    for result in stream.by_ref() {
        let value = result.map_err(|e| ToonedError::DecodeFailed(e.to_string()))?;

        if first {
            first = false;
            if let Value::Object(obj) = &value
                && let Ok(k) = object_keys(obj, 0)
                && object_values(obj, &k, 0).is_ok()
            {
                header = class_header("A", &k);
                out.write_all(header.as_bytes())
                    .map_err(|e| ToonedError::DecodeFailed(e.to_string()))?;
                keys = Some(k);
            }
            out.write_all(b"[").map_err(|e| ToonedError::DecodeFailed(e.to_string()))?;
            array_opened = true;
            let text = stream_value_text(&value, keys.as_ref())?;
            validate_stream_record(&header, keys.as_ref(), &text, &value)?;
            out.write_all(text.as_bytes()).map_err(|e| ToonedError::DecodeFailed(e.to_string()))?;
            continue;
        }

        out.write_all(b",").map_err(|e| ToonedError::DecodeFailed(e.to_string()))?;
        let text = stream_value_text(&value, keys.as_ref())?;
        validate_stream_record(&header, keys.as_ref(), &text, &value)?;
        out.write_all(text.as_bytes()).map_err(|e| ToonedError::DecodeFailed(e.to_string()))?;
    }

    if array_opened {
        out.write_all(b"]").map_err(|e| ToonedError::DecodeFailed(e.to_string()))?;
    } else {
        out.write_all(b"[]").map_err(|e| ToonedError::DecodeFailed(e.to_string()))?;
    }
    out.flush().map_err(|e| ToonedError::DecodeFailed(e.to_string()))?;

    let input_bytes = stream.bytes_read();
    let output_bytes = out.count;
    Ok(StreamStats { input_bytes, output_bytes })
}

/// Render a single NDJSON record as the TRON text that will appear inside the
/// body array. When a class schema has been established, compatible objects are
/// emitted as `A(...)` instances; everything else falls back to plain JSON.
fn stream_value_text(value: &Value, keys: Option<&Vec<String>>) -> Result<String, ToonedError> {
    if let Some(keys) = keys
        && let Value::Object(obj) = value
        && let Ok(vals) = object_values(obj, keys, 0)
    {
        return Ok(encode_instance(&vals));
    }
    sonic_rs::to_string(value).map_err(|e| {
        ToonedError::DecodeFailed(format!("failed to serialize TRON fallback value: {e}"))
    })
}

/// Verify that emitting `text` for `value` decodes back to exactly `value`.
/// This keeps the streaming path memory-bounded: no `Vec<Value>` of every
/// record is retained, yet we still fail closed on any lossy conversion.
fn validate_stream_record(
    header: &str,
    keys: Option<&Vec<String>>,
    text: &str,
    value: &Value,
) -> Result<(), ToonedError> {
    let test_doc = if keys.is_some() {
        let mut s = header.to_string();
        s.push('[');
        s.push_str(text);
        s.push(']');
        s
    } else {
        format!("[{text}]")
    };
    let decoded = decode(&test_doc)?;
    if decoded != Value::Array(vec![value.clone()]) {
        return Err(ToonedError::DecodeFailed(
            "streaming TRON record is not losslessly reversible".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_uniform_array_of_flat_objects() {
        let value = serde_json::json!([
            {"id": 0, "name": "row-0", "active": true, "score": 0.0},
            {"id": 1, "name": "row-1", "active": false, "score": 1.5},
        ]);
        let text = encode(&value).expect("encodable");
        let decoded = decode(&text).expect("decodable");
        assert_eq!(decoded, value);
    }

    #[test]
    fn encode_decode_single_object() {
        let value = serde_json::json!({"id": 42, "name": "item"});
        let text = encode(&value).expect("encodable");
        let decoded = decode(&text).expect("decodable");
        assert_eq!(decoded, value);
    }

    #[test]
    fn decode_plain_json_without_header() {
        let text = r#"[{"a": 1}, {"a": 2}]"#;
        let value = decode(text).expect("decodable");
        assert_eq!(value, serde_json::json!([{"a": 1}, {"a": 2}]));
    }

    #[test]
    fn decode_class_call_inside_json_object() {
        let text = "class A: x,y\n\n{\"items\": [A(1,\"one\"),A(2,\"two\")]}";
        let value = decode(text).expect("decodable");
        assert_eq!(
            value,
            serde_json::json!({"items": [{"x": 1, "y": "one"}, {"x": 2, "y": "two"}]})
        );
    }

    #[test]
    fn maybe_tron_converts_uniform_array_and_round_trips() {
        let input = br#"[{"id":0,"name":"row-0","active":true,"score":0.5},{"id":1,"name":"row-1","active":false,"score":1.5}]"#;
        let opts = ConversionOptions::default();
        let result = maybe_tron(input, &opts).expect("infallible for payload-driven input");
        assert!(matches!(result, Conversion::Toon { .. }));
    }

    #[test]
    fn maybe_tron_passes_through_scalar() {
        let input = b"42";
        let opts = ConversionOptions::default();
        let result = maybe_tron(input, &opts).expect("infallible");
        match result {
            Conversion::Passthrough { reason: PassthroughReason::NotStructuredData, .. } => {}
            other => panic!("expected Passthrough(NotStructuredData), got {other:?}"),
        }
    }

    #[test]
    fn maybe_tron_stream_round_trips_uniform_array() {
        let input = "{\"id\":0,\"name\":\"row-0\"}\n{\"id\":1,\"name\":\"row-1\"}\n";
        let mut out: Vec<u8> = Vec::new();
        maybe_tron_stream(std::io::Cursor::new(input), &mut out).expect("stream");
        let text = String::from_utf8(out).expect("utf8");
        // The streamed body must decode back to the original records.
        let decoded = decode(&text).expect("decodable");
        assert_eq!(decoded, serde_json::json!([{"id":0,"name":"row-0"},{"id":1,"name":"row-1"}]));
    }

    #[test]
    fn maybe_tron_stream_falls_back_on_corrupt_record() {
        // A record with a non-primitive value for a class column cannot be
        // faithfully represented as `A(...)`; the streaming path must fall back
        // to emitting the original JSON array rather than ship a lossy TOON.
        let input = "{\"id\":1,\"name\":\"a\"}\n{\"id\":2,\"nested\":{\"x\":1}}\n";
        let mut out: Vec<u8> = Vec::new();
        maybe_tron_stream(std::io::Cursor::new(input), &mut out).expect("stream");
        let text = String::from_utf8(out).expect("utf8");
        let decoded = decode(&text).expect("decodable");
        // The fallback must still reconstruct the original records exactly.
        assert_eq!(decoded, serde_json::json!([{"id":1,"name":"a"},{"id":2,"nested":{"x":1}}]));
    }
}
