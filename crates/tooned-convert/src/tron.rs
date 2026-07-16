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

use serde_json::Value;
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

    let mut out = String::new();
    out.push_str(CLASS_PREFIX);
    out.push('A');
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

    if let Value::Array(_) = value {
        out.push('[');
        for (i, row) in values.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            append_instance(&mut out, row);
        }
        out.push(']');
    } else {
        // Single top-level object.
        let row = values
            .first()
            .ok_or_else(|| ToonedError::DecodeFailed("TRON missing single object row".into()))?;
        append_instance(&mut out, row);
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

fn object_row(
    obj: &serde_json::Map<String, Value>,
    index: usize,
) -> Result<(Vec<String>, Vec<String>), ToonedError> {
    if obj.is_empty() {
        return Err(ToonedError::DecodeFailed(format!("TRON object {index} has no keys")));
    }

    let mut keys = Vec::with_capacity(obj.len());
    let mut vals = Vec::with_capacity(obj.len());
    for (key, value) in obj {
        if !is_valid_header_key(key) {
            return Err(ToonedError::DecodeFailed(format!(
                "TRON object key {key:?} is not a valid header identifier"
            )));
        }
        if !is_primitive(value) {
            return Err(ToonedError::DecodeFailed(format!(
                "TRON object {index}, key {key} contains a non-primitive value"
            )));
        }
        keys.push(key.clone());
        vals.push(serde_json::to_string(value).map_err(|e| {
            ToonedError::DecodeFailed(format!("failed to serialize TRON cell: {e}"))
        })?);
    }
    Ok((keys, vals))
}

fn append_instance(out: &mut String, values: &[String]) {
    out.push('A');
    out.push('(');
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(v);
    }
    out.push(')');
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
        serde_json::from_str(body.trim())
            .map_err(|e| ToonedError::DecodeFailed(format!("TRON body is not valid JSON: {e}")))
    } else {
        let json_text = expand_tron(body, &classes)?;
        serde_json::from_str(&json_text).map_err(|e| {
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
    serde_json::to_writer(&mut counter, &value).map_err(|e| {
        ToonedError::DecodeFailed(format!("failed to compute JSON size for TRON comparison: {e}"))
    })?;
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
}
