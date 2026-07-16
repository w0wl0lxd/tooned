// SPDX-License-Identifier: AGPL-3.0-only

//! Prototype ONTO (Object-Notation Tabular Output) encoder/decoder.
//!
//! ONTO is a prototype columnar, pipe-delimited encoding for uniform arrays of
//! flat objects. The schema is emitted once; each subsequent line is a row.
//! Values are encoded as JSON literals, so strings are quoted and JSON-escaped
//! and primitives (`null`, booleans, numbers) are emitted bare.
//!
//! ```text
//! !schema "id"|"name"|"active"|"score"
//! 0|"row-0"|true|0
//! ```
//!
//! This is an experimental first step toward the ONTO/TRON encoding family.
//! It only handles uniform arrays of flat objects and falls back to
//! passthrough for everything else.

use serde_json::Value;
use tooned_types::{
    Conversion, ConversionOptions, ConversionReport, PassthroughReason, ToonedError,
};

use crate::shape;

const SCHEMA_PREFIX: &str = "!schema ";

fn is_primitive(value: &Value) -> bool {
    !matches!(value, Value::Object(_) | Value::Array(_))
}

fn sorted_keys(obj: &serde_json::Map<String, Value>) -> Vec<&str> {
    let mut keys: Vec<&str> = obj.keys().map(std::string::String::as_str).collect();
    keys.sort_unstable();
    keys
}

fn quote_json(value: &Value) -> Result<String, ToonedError> {
    serde_json::to_string(value)
        .map_err(|e| ToonedError::DecodeFailed(format!("failed to serialize ONTO cell: {e}")))
}

/// Encode a uniform array of flat objects as ONTO text.
pub fn encode(value: &Value) -> Result<String, ToonedError> {
    let Value::Array(arr) = value else {
        return Err(ToonedError::DecodeFailed("ONTO requires a top-level array".into()));
    };
    if arr.is_empty() {
        return Err(ToonedError::DecodeFailed("ONTO cannot encode an empty array".into()));
    }

    let Some(first) = arr.first() else {
        return Err(ToonedError::DecodeFailed("ONTO array is unexpectedly empty".into()));
    };
    let Value::Object(first_obj) = first else {
        return Err(ToonedError::DecodeFailed("ONTO array elements must be objects".into()));
    };
    let keys = sorted_keys(first_obj);
    if keys.is_empty() {
        return Err(ToonedError::DecodeFailed("ONTO objects must have at least one key".into()));
    }

    for (i, item) in arr.iter().enumerate() {
        let Value::Object(obj) = item else {
            return Err(ToonedError::DecodeFailed(format!(
                "ONTO array element {i} is not an object"
            )));
        };
        if sorted_keys(obj) != keys {
            return Err(ToonedError::DecodeFailed(format!(
                "ONTO array element {i} has a different key set than the first element"
            )));
        }
        for key in &keys {
            if !is_primitive(&obj[*key]) {
                return Err(ToonedError::DecodeFailed(format!(
                    "ONTO array element {i}, key {key} is not a primitive value"
                )));
            }
        }
    }

    let mut out = String::new();
    out.push_str(SCHEMA_PREFIX);
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            out.push('|');
        }
        out.push_str(&quote_json(&Value::String((*key).to_string()))?);
    }
    out.push('\n');

    for item in arr {
        let Some(obj) = item.as_object() else {
            return Err(ToonedError::DecodeFailed("ONTO validated object disappeared".into()));
        };
        for (i, key) in keys.iter().enumerate() {
            if i > 0 {
                out.push('|');
            }
            out.push_str(&quote_json(&obj[*key])?);
        }
        out.push('\n');
    }

    Ok(out)
}

fn split_cells(line: &str) -> Result<Vec<&str>, ToonedError> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_quote = false;
    let mut escape = false;

    for (i, c) in line.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if in_quote {
            if c == '"' {
                in_quote = false;
            }
            continue;
        }
        if c == '"' {
            in_quote = true;
            continue;
        }
        if c == '|' {
            out.push(&line[start..i]);
            start = i + c.len_utf8();
        }
    }

    if in_quote || escape {
        return Err(ToonedError::DecodeFailed("unterminated quoted ONTO cell".into()));
    }
    out.push(&line[start..]);
    Ok(out)
}

/// Decode ONTO text back to a `serde_json::Value`.
pub fn decode(text: &str) -> Result<Value, ToonedError> {
    let mut lines = text.lines();
    let first =
        lines.next().ok_or_else(|| ToonedError::DecodeFailed("ONTO input is empty".into()))?;
    if !first.starts_with(SCHEMA_PREFIX) {
        return Err(ToonedError::DecodeFailed(
            "ONTO input does not start with a !schema header".into(),
        ));
    }

    let key_cells = split_cells(&first[SCHEMA_PREFIX.len()..])?;
    let keys: Vec<String> = key_cells
        .iter()
        .map(|c| {
            serde_json::from_str::<String>(c).map_err(|e| {
                ToonedError::DecodeFailed(format!("invalid ONTO schema key {c:?}: {e}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    if keys.is_empty() {
        return Err(ToonedError::DecodeFailed("ONTO schema has no keys".into()));
    }

    let mut rows = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let cells = split_cells(line)?;
        if cells.len() != keys.len() {
            return Err(ToonedError::DecodeFailed(format!(
                "ONTO row has {} cells, expected {}",
                cells.len(),
                keys.len()
            )));
        }

        let mut obj = serde_json::Map::new();
        for (key, cell) in keys.iter().zip(cells.iter()) {
            let value: Value = serde_json::from_str(cell).map_err(|e| {
                ToonedError::DecodeFailed(format!("invalid ONTO cell {cell:?}: {e}"))
            })?;
            obj.insert(key.clone(), value);
        }
        rows.push(Value::Object(obj));
    }

    Ok(Value::Array(rows))
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

/// Run the full detect/parse/encode/margin/round-trip pipeline for ONTO.
///
/// Mirrors `maybe_tooned` semantics: payload-driven failures downgrade to
/// `Conversion::Passthrough`, never `Err`.
pub fn maybe_onto(input: &[u8], opts: &ConversionOptions) -> Result<Conversion, ToonedError> {
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

    let value = crate::parse_by_doc_type(input, doc_type)
        .map_err(|_| ToonedError::DecodeFailed("ONTO parse failed".into()))?;

    let shape = shape::classify(&value);

    let mut counter = ByteCountingWriter(0);
    serde_json::to_writer(&mut counter, &value).map_err(|e| {
        ToonedError::DecodeFailed(format!("failed to compute JSON size for ONTO comparison: {e}"))
    })?;
    let json_bytes = counter.0;

    let Ok(encoded) = encode(&value) else {
        return Ok(Conversion::Passthrough {
            bytes: input.to_vec(),
            reason: PassthroughReason::ParseFailed,
        });
    };
    let onto_bytes = encoded.len();

    if !crate::is_smaller_enough(json_bytes, onto_bytes, opts.margin_pct) {
        return Ok(Conversion::Passthrough {
            bytes: input.to_vec(),
            reason: PassthroughReason::NotSmallerEnough { json_bytes, toon_bytes: onto_bytes },
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
            toon_bytes: onto_bytes,
            savings_pct: crate::compute_savings_pct(json_bytes, onto_bytes),
        },
    })
}
