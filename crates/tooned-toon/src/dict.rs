// SPDX-License-Identifier: AGPL-3.0-only

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

    // O(1) per-line membership check; `data_indices.contains` inside the later
    // line loop would be O(n) per line and dominates on large tabular payloads.
    let mut is_data = vec![false; lines.len()];
    for &di in &data_indices {
        if let Some(cell) = is_data.get_mut(di) {
            *cell = true;
        }
    }

    let protected_lower: HashSet<String> =
        protected_keys.iter().map(|p| p.to_ascii_lowercase()).collect();

    let protected_idx: HashSet<usize> = if object_mode {
        HashSet::new()
    } else {
        keys.iter()
            .enumerate()
            .filter(|(_, k)| key_is_protected(k, &protected_lower))
            .map(|(i, _)| i)
            .collect()
    };

    // Frequency of each cell token across data lines (skipping protected
    // columns/keys so critical values stay verbatim). Keys borrow from `lines`
    // to avoid cloning every token before we know any of them will compress.
    let mut freq: HashMap<&str, usize> =
        HashMap::with_capacity(data_indices.len().saturating_mul(4));
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
                if key_is_protected(key, &protected_lower) {
                    continue;
                }
                for cell in split_cells(val) {
                    *freq.entry(cell).or_insert(0) += 1;
                }
            } else {
                for cell in split_cells(line) {
                    *freq.entry(cell).or_insert(0) += 1;
                }
            }
        } else {
            for (col, cell) in split_cells(line).into_iter().enumerate() {
                if !protected_idx.contains(&col) {
                    *freq.entry(cell).or_insert(0) += 1;
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
        mapping.push((token.to_string(), sentinel));
    }
    if mapping.is_empty() {
        return None;
    }

    let map: HashMap<&str, &str> = mapping.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();

    // Net-positive compression guarantees the output is no larger than the
    // input, so `toon.len()` is a safe initial capacity.
    let mut out = String::with_capacity(toon.len());
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
        if object_mode {
            if line.trim().is_empty() {
                // leave blank
            } else if is_data.get(li).is_some_and(|&b| b) {
                transform_line(line, &map, true, &mut out);
            } else {
                out.push_str(line);
            }
        } else if li == header_idx {
            out.push_str(line);
        } else if line.trim().is_empty() {
            // leave blank
        } else if is_data.get(li).is_some_and(|&b| b) {
            transform_line(line, &map, false, &mut out);
        } else {
            out.push_str(line);
        }
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

    let use_crlf = text.contains("\r\n");
    let eol = if use_crlf { "\r\n" } else { "\n" };

    let lines: Vec<&str> = text
        .split('\n')
        .map(|s| if let Some(stripped) = s.strip_suffix('\r') { stripped } else { s })
        .collect();
    let mut map: HashMap<&str, &str> = HashMap::new();
    let mut i = 1;
    while let Some(line) = lines.get(i).copied() {
        if line.trim().is_empty() {
            i += 1;
            break;
        }
        if let Some(sp) = line.find(' ') {
            let sentinel = or_fallback(line.get(..sp), "");
            let original = or_fallback(line.get(sp + 1..), "");
            if !sentinel.is_empty() {
                map.insert(sentinel, original);
            }
        }
        i += 1;
    }

    let mut out = String::with_capacity(text.len());
    let mut lines_iter = lines.iter().skip(i).peekable();
    while let Some(&line) = lines_iter.next() {
        expand_line(line, &map, &mut out);
        if out.len() > max_output_bytes {
            return Err(ToonedError::InputTooLarge);
        }
        if lines_iter.peek().is_some() {
            if out.len() + eol.len() > max_output_bytes {
                return Err(ToonedError::InputTooLarge);
            }
            out.push_str(eol);
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
    let mut cells = Vec::with_capacity(8);
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

/// Append the cells of `s` to `out`, replacing any token that appears in `map`
/// with its sentinel. `map` is token -> sentinel.
fn replace_cells_into(s: &str, map: &HashMap<&str, &str>, out: &mut String) {
    let cells = split_cells(s);
    for (i, c) in cells.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(or_fallback(map.get(c).copied(), c));
    }
}

/// Append the cells of `s` to `out`, replacing any sentinel that appears in
/// `map` with its original token. `map` is sentinel -> original.
fn replace_cells_expand_into(s: &str, map: &HashMap<&str, &str>, out: &mut String) {
    let cells = split_cells(s);
    for (i, c) in cells.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(or_fallback(map.get(c).copied(), c));
    }
}

/// Replace mapped cell tokens in `line` with their sentinels (array mode) or
/// mapped value tokens (object mode `key: value`). Writes directly to `out`.
fn transform_line(line: &str, map: &HashMap<&str, &str>, object_mode: bool, out: &mut String) {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    out.push_str(&line[..indent]);
    if object_mode {
        if let Some(sp) = trimmed.find(": ") {
            let key = or_fallback(trimmed.get(..sp), "");
            let val = or_fallback(trimmed.get(sp + 2..), "");
            out.push_str(key);
            out.push_str(": ");
            if let Some(sent) = map.get(val).copied() {
                out.push_str(sent);
            } else {
                replace_cells_into(val, map, out);
            }
        } else {
            replace_cells_into(trimmed, map, out);
        }
    } else {
        replace_cells_into(trimmed, map, out);
    }
}

/// Replace sentinel tokens in `line` with their originals. Writes directly to `out`.
fn expand_line(line: &str, map: &HashMap<&str, &str>, out: &mut String) {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    out.push_str(&line[..indent]);
    if let Some(sp) = trimmed.find(": ") {
        let key = or_fallback(trimmed.get(..sp), "");
        let val = or_fallback(trimmed.get(sp + 2..), "");
        out.push_str(key);
        out.push_str(": ");
        if let Some(orig) = map.get(val).copied() {
            out.push_str(orig);
        } else {
            replace_cells_expand_into(val, map, out);
        }
    } else {
        replace_cells_expand_into(trimmed, map, out);
    }
}

/// Detect whether `toon` is an array-of-objects table (header line
/// `[N]{k1,k2,...}:`) or a single-object document (`key: value` lines).
/// Returns `(object_mode, header_index, header_keys)`; for object mode
/// `header_index` is 0 and `header_keys` is empty.
fn find_structure<'a>(lines: &'a [&'a str]) -> (bool, usize, Vec<&'a str>) {
    for (i, line) in lines.iter().enumerate() {
        let l = line.trim();
        if l.contains('{')
            && l.contains('}')
            && l.ends_with(':')
            && let (Some(a), Some(b)) = (l.find('{'), l.rfind('}'))
            && a < b
        {
            let inner = or_fallback(l.get(a + 1..b), "");
            let keys: Vec<&str> =
                inner.split(',').map(|s| s.trim().trim_start_matches('@')).collect();
            return (false, i, keys);
        }
    }
    (true, 0, Vec::new())
}

/// Case-insensitive substring protection check between a TOON header
/// key and the set of configured protected key names.
fn key_is_protected(header_key: &str, protected_lower: &HashSet<String>) -> bool {
    let header_key_lower = header_key.to_ascii_lowercase();
    protected_lower.iter().any(|p| header_key_lower.contains(p))
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
        let toon =
            "a: 1\nb: this_is_a_very_long_repeated_value\nc: this_is_a_very_long_repeated_value\n";
        let protected: Vec<String> = vec![];
        let dict = apply_dict(toon, &protected).expect("should compress");
        assert_eq!(expand_legend(&dict, usize::MAX).unwrap(), toon);
    }

    #[test]
    fn protects_critical_columns() {
        let toon = "[4]{id,role}:\n\n  1,administrator\n  2,administrator\n  3,administrator\n  4,administrator\n";
        let protected = vec!["role".to_string()];
        let dict = apply_dict(toon, &protected);
        assert!(dict.is_none(), "protected column must not be compressed");
    }

    #[test]
    fn round_trips_crlf_line_endings() {
        let toon = "[8]{id,name,role}:\r\n\r\n  1,Alice,administrator\r\n  2,Bob,administrator\r\n  3,Cara,administrator\r\n  4,Dan,administrator\r\n  5,Eve,administrator\r\n  6,Fay,administrator\r\n  7,Gus,administrator\r\n  8,Hal,administrator\r\n";
        let protected: Vec<String> = vec![];
        let dict = apply_dict(toon, &protected).expect("should compress");
        assert!(dict.contains("\r\n"), "compressed output must preserve CRLF");
        let expanded = expand_legend(&dict, usize::MAX).unwrap();
        assert_eq!(expanded, toon, "CRLF round trip must be lossless");
    }

    #[test]
    fn object_mode_fallback_preserves_key() {
        let toon = "note: repeated_long_token, repeated_long_token\r\nrole: repeated_long_token, repeated_long_token\r\n";
        let protected: Vec<String> = vec![];
        let dict = apply_dict(toon, &protected).expect("should compress");
        let expanded = expand_legend(&dict, usize::MAX).unwrap();
        assert_eq!(expanded, toon, "object-mode value fallback must not corrupt key");
    }
}
