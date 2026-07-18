//! Compression via Dictionary-Encoding").
//!
//! TOON already lifts repeated *keys* into a header; this tier lifts repeated
//! *values* into a `legend:` block. Every repeated cell token is replaced by a
//! short sentinel string recorded in the legend. The transform is purely
//! textual (it never touches the structure the external `toon-lsp` codec
//! owns), strictly net-win gated, and reversed exactly by [`expand_legend`]
//! -- which [`crate::decode_toon_with_limit`] calls before delegating to the
//! codec. The conversion pipeline additionally verifies the full round trip
//! before any dictionary-encoded output is ever surfaced, so a malformed
//! payload can never be emitted as a corrupted conversion (constitution
//! Principle I).

use std::collections::{HashMap, HashSet};

use tooned_types::ToonedError;

/// Explicit fallback for `Option<&str>` that avoids the banned
/// `Option::unwrap_or` method while keeping call sites compact.
fn or_fallback<'a>(opt: Option<&'a str>, fallback: &'a str) -> &'a str {
    if let Some(s) = opt { s } else { fallback }
}

/// Private-use character used to build sentinels and the legend marker. It
/// cannot appear in real data, so a sentinel can never collide with a literal
/// cell value (which would be quoted if it contained unusual characters).
const SENTINEL_PREFIX: char = '\u{E000}';

/// First line of a dictionary-encoded document. Contains the private-use
/// character so it can never collide with a real TOON key named `legend`.
const LEGEND_MARKER: &str = "\u{E000}legend:";

/// Try to dictionary-compress `toon`. Returns `None` when there is no net
/// saving (caller keeps the original TOON). `protected_keys` are key names
/// whose columns must never be abbreviated (critical-field policy); pass an
/// empty slice when no column is protected.
#[must_use]
pub fn apply_dict(toon: &str, protected_keys: &[String]) -> Option<String> {
    // The sentinel private-use character must not already appear in the input,
    // or the compressed output could collide with literal data.
    if toon.contains(SENTINEL_PREFIX) {
        return None;
    }

    let use_crlf = toon.contains("\r\n");
    let eol = if use_crlf { "\r\n" } else { "\n" };

    let lines: Vec<&str> = toon
        .split('\n')
        .map(|s| if let Some(stripped) = s.strip_suffix('\r') { stripped } else { s })
        .collect();
    let (object_mode, header_idx, keys) = find_structure(&lines);

    let data_indices: Vec<usize> = if object_mode {
        (0..lines.len()).filter(|&i| lines.get(i).is_some_and(|l| !l.trim().is_empty())).collect()
    } else {
        ((header_idx + 1)..lines.len())
            .filter(|&i| lines.get(i).is_some_and(|l| !l.trim().is_empty()))
            .collect()
    };
    if data_indices.is_empty() {
        return None;
    }

    let protected_idx: HashSet<usize> = if object_mode {
        HashSet::new()
    } else {
        keys.iter()
            .enumerate()
            .filter(|(_, k)| protected_keys.iter().any(|p| key_protected(k, p)))
            .map(|(i, _)| i)
            .collect()
    };

    // Frequency of each cell token across data lines (skipping protected
    // columns/keys so critical values stay verbatim).
    let mut freq: HashMap<String, usize> = HashMap::new();
    for &di in &data_indices {
        let line = match lines.get(di) {
            Some(l) => l.trim(),
            None => continue,
        };
        if object_mode {
            // Object-mode "key: value" lines: protect by key name, then
            // frequency-count cells inside the value.
            if let Some(sp) = line.find(": ") {
                let key = or_fallback(line.get(..sp), "");
                let val = or_fallback(line.get(sp + 2..), "");
                if protected_keys.iter().any(|p| key_protected(key, p)) {
                    continue;
                }
                for cell in split_cells(val) {
                    *freq.entry(cell.to_string()).or_insert(0) += 1;
                }
            } else {
                for cell in split_cells(line) {
                    *freq.entry(cell.to_string()).or_insert(0) += 1;
                }
            }
        } else {
            for (col, cell) in split_cells(line).into_iter().enumerate() {
                if !protected_idx.contains(&col) {
                    *freq.entry(cell.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    // Select tokens worth substituting: repeated, longer than their sentinel,
    // and individually net-positive (saving must exceed the legend line cost).
    let mut mapping: Vec<(String, String)> = Vec::new();
    for (token, count) in freq {
        if count < 2 {
            continue;
        }
        let tok_len = token.len();
        let idx = mapping.len();
        let sentinel = format!("{SENTINEL_PREFIX}{idx}");
        let s_len = sentinel.len();
        if tok_len <= s_len {
            continue;
        }
        let saving = (tok_len - s_len) * count;
        let entry_cost = s_len + 1 + tok_len + 1;
        if saving <= entry_cost {
            continue;
        }
        mapping.push((token, sentinel));
    }
    if mapping.is_empty() {
        return None;
    }

    let map: HashMap<&str, &str> = mapping.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();

    let mut out = String::new();
    out.push_str(LEGEND_MARKER);
    out.push_str(eol);
    for (orig, sent) in &mapping {
        out.push_str(sent);
        out.push(' ');
        out.push_str(orig);
        out.push_str(eol);
    }
    out.push_str(eol);

    let total = lines.len();
    for (li, line) in lines.iter().enumerate() {
        let is_last = li + 1 == total;
        let emit_eol = !(is_last && line.trim().is_empty());
        let content: String = if object_mode {
            if line.trim().is_empty() {
                String::new()
            } else if data_indices.contains(&li) {
                transform_line(line, &map, true)
            } else {
                line.to_string()
            }
        } else if li == header_idx {
            line.to_string()
        } else if line.trim().is_empty() {
            String::new()
        } else if data_indices.contains(&li) {
            transform_line(line, &map, false)
        } else {
            line.to_string()
        };
        out.push_str(&content);
        if emit_eol {
            out.push_str(eol);
        }
    }

    Some(out)
}

/// Reverse [`apply_dict`]: if `text` begins with the legend marker, expand the
/// sentinel references back to their original cell tokens and return the plain
/// TOON document. Returns `text` unchanged when there is no legend.
///
/// `max_output_bytes` bounds the expanded output so a small encoded payload
/// with a huge legend cannot allocate an unbounded string.
pub fn expand_legend(text: &str, max_output_bytes: usize) -> Result<String, ToonedError> {
    if !text.starts_with(LEGEND_MARKER) {
        if text.len() > max_output_bytes {
            return Err(ToonedError::InputTooLarge);
        }
        return Ok(text.to_string());
    }

    let lines: Vec<&str> = text
        .split('\n')
        .map(|s| if let Some(stripped) = s.strip_suffix('\r') { stripped } else { s })
        .collect();
    let mut map: HashMap<String, String> = HashMap::new();
    let mut i = 1;
    while let Some(line) = lines.get(i).copied() {
        if line.trim().is_empty() {
            i += 1;
            break;
        }
        if let Some(sp) = line.find(' ') {
            let sentinel = or_fallback(line.get(..sp), "").to_string();
            let original = or_fallback(line.get(sp + 1..), "").to_string();
            if !sentinel.is_empty() {
                map.insert(sentinel, original);
            }
        }
        i += 1;
    }

    let mut out = String::new();
    let mut lines_iter = lines.iter().skip(i).peekable();
    while let Some(&line) = lines_iter.next() {
        let expanded = expand_line(line, &map);
        if out.len() + expanded.len() > max_output_bytes {
            return Err(ToonedError::InputTooLarge);
        }
        out.push_str(&expanded);
        if lines_iter.peek().is_some() {
            if out.len() + 1 > max_output_bytes {
                return Err(ToonedError::InputTooLarge);
            }
            out.push('\n');
        }
    }
    Ok(out)
}

/// Split a TOON data line into cell tokens, respecting quoted strings (so a
/// comma inside `"a,b"` is not treated as a separator). Returns borrowed
/// substrings of `line`.
fn split_cells(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let n = bytes.len();
    let mut cells = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < n {
        match bytes.get(i).copied() {
            Some(b'"') => {
                i += 1;
                while i < n {
                    match bytes.get(i).copied() {
                        Some(b'\\') => i = i.saturating_add(2),
                        Some(b'"') => break,
                        _ => i += 1,
                    }
                }
                i = i.saturating_add(1);
            }
            Some(b',') => {
                cells.push(or_fallback(line.get(start..i), ""));
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    cells.push(or_fallback(line.get(start..n), ""));
    cells
}

/// Replace mapped cell tokens in `line` with their sentinels (array mode) or
/// mapped value tokens (object mode `key: value`).
fn transform_line(line: &str, map: &HashMap<&str, &str>, object_mode: bool) -> String {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    let body = if object_mode {
        if let Some(sp) = trimmed.find(": ") {
            let key = or_fallback(trimmed.get(..sp), "");
            let val = or_fallback(trimmed.get(sp + 2..), "");
            if let Some(s) = map.get(val) {
                format!("{key}: {s}")
            } else {
                format!("{key}: {}", replace_cells(val, map))
            }
        } else {
            replace_cells(trimmed, map)
        }
    } else {
        replace_cells(trimmed, map)
    };
    let mut out = String::with_capacity(indent + body.len());
    out.push_str(&line[..indent]);
    out.push_str(&body);
    out
}

/// Replace sentinel tokens in `line` with their originals.
fn expand_line(line: &str, map: &HashMap<String, String>) -> String {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    let body = if let Some(sp) = trimmed.find(": ") {
        let key = or_fallback(trimmed.get(..sp), "");
        let val = or_fallback(trimmed.get(sp + 2..), "");
        if let Some(orig) = map.get(val) {
            format!("{key}: {orig}")
        } else {
            replace_cells_expand(trimmed, map)
        }
    } else {
        replace_cells_expand(trimmed, map)
    };
    let mut out = String::with_capacity(indent + body.len());
    out.push_str(&line[..indent]);
    out.push_str(&body);
    out
}

fn replace_cells(s: &str, map: &HashMap<&str, &str>) -> String {
    let cells = split_cells(s);
    let mut out = String::new();
    for (i, c) in cells.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(or_fallback(map.get(c).copied(), c));
    }
    out
}

fn replace_cells_expand(s: &str, map: &HashMap<String, String>) -> String {
    let cells = split_cells(s);
    let mut out = String::new();
    for (i, c) in cells.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(map.get(*c).map_or(*c, String::as_str));
    }
    out
}

/// Detect whether `toon` is an array-of-objects table (header line
/// `[N]{k1,k2,...}:`) or a single-object document (`key: value` lines).
/// Returns `(object_mode, header_index, header_keys)`; for object mode
/// `header_index` is 0 and `header_keys` is empty.
fn find_structure(lines: &[&str]) -> (bool, usize, Vec<String>) {
    for (i, line) in lines.iter().enumerate() {
        let l = line.trim();
        if l.contains('{')
            && l.contains('}')
            && l.ends_with(':')
            && let (Some(a), Some(b)) = (l.find('{'), l.rfind('}'))
            && a < b
        {
            let inner = or_fallback(l.get(a + 1..b), "");
            let keys: Vec<String> =
                inner.split(',').map(|s| s.trim().trim_start_matches('@').to_string()).collect();
            return (false, i, keys);
        }
    }
    (true, 0, Vec::new())
}

/// Case-insensitive substring protection check between a TOON header key and a
/// configured protected key name.
fn key_protected(header_key: &str, protected: &str) -> bool {
    header_key.to_ascii_lowercase().contains(&protected.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn expand_without_legend_is_identity() {
        let toon = "[2]{id,name}:\n\n  1,\"Alice Chen\"\n  2,\"Bob Diaz\"\n";
        assert_eq!(expand_legend(toon, usize::MAX).unwrap(), toon);
    }

    #[test]
    fn no_benefit_returns_none() {
        let toon = "[2]{id,name}:\n\n  1,\"Alice Chen\"\n  2,\"Bob Diaz\"\n";
        let protected: Vec<String> = vec![];
        assert!(apply_dict(toon, &protected).is_none());
    }
    #[test]
    fn round_trips_array_with_repeated_values() {
        let toon = "[8]{id,name,role}:

  1,Alice,administrator
  2,Bob,administrator
  3,Cara,administrator
  4,Dan,administrator
  5,Eve,administrator
  6,Fay,administrator
  7,Gus,administrator
  8,Hal,administrator
";
        let protected: Vec<String> = vec![];
        let dict = apply_dict(toon, &protected).expect("should compress");
        assert!(dict.starts_with(LEGEND_MARKER));
        let expanded = expand_legend(&dict, usize::MAX).unwrap();
        assert_eq!(expanded, toon, "legend expansion must be lossless");
    }

    #[test]
    fn round_trips_object_document() {
        let toon = "a: 1
b: this_is_a_very_long_repeated_value
c: this_is_a_very_long_repeated_value
";
        let protected: Vec<String> = vec![];
        let dict = apply_dict(toon, &protected).expect("should compress");
        assert_eq!(expand_legend(&dict, usize::MAX).unwrap(), toon);
    }

    #[test]
    fn protects_critical_columns() {
        let toon = "[4]{id,role}:

  1,administrator
  2,administrator
  3,administrator
  4,administrator
";
        let protected = vec!["role".to_string()];
        let dict = apply_dict(toon, &protected);
        assert!(dict.is_none(), "protected column must not be compressed");
    }
}
