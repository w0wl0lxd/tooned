// SPDX-License-Identifier: AGPL-3.0-only

//! Lossless dictionary compression tier for TOON text (#1).
//!
//! TOON is already columnar/compact, but real-world payloads repeat the same
//! long scalar values (status strings, enum members, warehouse/SKU tags,
//! UUID-shaped IDs kept verbatim by the critical-field policy, ...) across
//! thousands of rows. This tier extracts a *legend* — a private-use-sentinel
//! dictionary — and substitutes it for the most frequent repeated cells,
//! mirroring the token-dictionary approach of recent lossless prompt-
//! compression work (e.g. arXiv:2604.13066).
//!
//! Everything here is purely a *text-layer* transform layered on top of
//! standard TOON: the upstream `toon-lsp` codec never sees it, because
//! `decode_toon_with_limit` calls [`expand_legend`] *before* the codec. The
//! tier is therefore fully lossless — [`expand_legend`] is a strict inverse of
//! [`apply_dict`] — and is only ever applied when it strictly shrinks the
//! total bytes (the net-win gate inside [`apply_dict`]) and the conversion
//! pipeline's own round-trip check still passes.

use std::collections::HashMap;

/// Private-use-area prefix for dictionary sentinels. Chosen from the Unicode
/// private use area so a sentinel can never collide with a real cell value
/// (user data is never in this range), keeping replacement unambiguous.
const SENTINEL_PREFIX: char = '\u{E000}';

/// Marker line that opens a legend block. Placed at the very top of the
/// dictionary-wrapped TOON text.
const LEGEND_MARKER: &str = "\u{E000}legend:";

/// Applies the dictionary tier to already-encoded TOON text.
///
/// Returns `None` (no transformation) when: the input is not a structured
/// TOON document we can parse, no token repeats enough to overcome the legend
/// overhead, or the net result would not be strictly smaller than `toon`.
/// `protected_keys` lists column/key names that must be left verbatim
/// (critical-field policy, #3).
pub fn apply_dict(toon: &str, protected_keys: &[String]) -> Option<String> {
    let mut token_counts: HashMap<&str, usize> = HashMap::new();

    let is_protected = |name: &str| -> bool {
        let lower = name.to_lowercase();
        protected_keys.iter().any(|p| lower.contains(p.as_str()))
    };

    let (columns, array_rows, object_entries) = scan_structure(toon);

    if let Some(cols) = &columns {
        for (ci, col) in cols.iter().enumerate() {
            if col.is_empty() || is_protected(col) {
                continue;
            }
            for row in &array_rows {
                if let Some(cell) = row.get(ci) {
                    *token_counts.entry(*cell).or_insert(0) += 1;
                }
            }
        }
    } else {
        for (key, val) in &object_entries {
            if key.is_empty() || is_protected(key) {
                continue;
            }
            *token_counts.entry(*val).or_insert(0) += 1;
        }
    }

    // Pick tokens whose total saving overcomes the single legend-entry cost.
    // Sentinels are short barewords, so we estimate the sentinel length at 4
    // bytes (prefix 3 + single digit) for selection.
    let est_sentinel_len = format!("{SENTINEL_PREFIX}0").len();
    let mut qualifying: Vec<(&str, usize)> = Vec::new();
    for (tok, &count) in &token_counts {
        if count < 2 {
            continue;
        }
        let saving = tok.len().saturating_sub(est_sentinel_len);
        let total_saving = saving * count;
        let entry_cost = est_sentinel_len + 1 + tok.len() + 1;
        if total_saving > entry_cost {
            qualifying.push((*tok, count));
        }
    }
    if qualifying.is_empty() {
        return None;
    }

    // Assign concrete sentinel indices now that we know the set.
    let mut abbrev: HashMap<&str, String> = HashMap::new();
    let mut legend_lines: Vec<(&str, String)> = Vec::new();
    for (tok, _count) in qualifying {
        let idx = legend_lines.len();
        let sentinel = format!("{SENTINEL_PREFIX}{idx}");
        // Re-verify with the real sentinel length (cheap; never changes the set
        // for idx < 10, but keeps correctness if a payload somehow needs >10).
        let saving = tok.len().saturating_sub(sentinel.len());
        let total_saving = saving * token_counts[tok];
        let entry_cost = sentinel.len() + 1 + tok.len() + 1;
        if total_saving > entry_cost {
            abbrev.insert(tok, sentinel.clone());
            legend_lines.push((tok, sentinel));
        }
    }
    if legend_lines.is_empty() {
        return None;
    }

    let mut out = String::new();
    out.push_str(LEGEND_MARKER);
    for (tok, sentinel) in &legend_lines {
        out.push('\n');
        out.push_str(sentinel);
        out.push(' ');
        out.push_str(tok);
    }
    out.push('\n');

    let transformed =
        transform_cells(toon, &columns, &array_rows, &object_entries, &abbrev, &is_protected);
    out.push_str(&transformed);

    // Net-win gate: never ship a dictionary-wrapped document that is not
    // strictly smaller than the plain TOON it replaces.
    if out.len() >= toon.len() {
        return None;
    }
    Some(out)
}

/// Reverses a [`apply_dict`] transform. Safe to call on plain TOON (no legend):
/// it is then a strict identity. The legend block is parsed away and every
/// sentinel cell replaced by its original value before the text reaches the
/// `toon-lsp` codec.
pub fn expand_legend(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut map: HashMap<&str, &str> = HashMap::new();
    let mut out = String::new();
    let mut in_legend = false;

    for line in lines {
        if !in_legend && line.starts_with(LEGEND_MARKER) {
            in_legend = true;
            continue;
        }
        if in_legend {
            // The legend block is terminated by the first line that does not
            // start with the private-use sentinel prefix (real data never
            // does), so no blank-line sentinel is required between legend and
            // TOON body.
            if line.starts_with(SENTINEL_PREFIX) {
                if let Some(sp) = line.find(' ') {
                    let sentinel = &line[..sp];
                    let original = &line[sp + 1..];
                    map.insert(sentinel, original);
                }
                continue;
            }
            in_legend = false;
        }
        // Object documents keep the sentinel in the value position
        // (`key: <sentinel>`); array rows keep it as a whole cell. Handle both.
        if let Some(colon) = line.find(": ") {
            let key = &line[..colon];
            let val = &line[colon + 2..];
            if let Some(original) = map.get(val) {
                out.push_str(key);
                out.push_str(": ");
                out.push_str(original);
                out.push('\n');
            } else {
                out.push_str(line);
                out.push('\n');
            }
        } else {
            out.push_str(&replace_cells_expand(line, &map));
            out.push('\n');
        }
    }
    out
}

/// Result of parsing a TOON document's high-level structure. `columns` is
/// `Some` for array-of-objects documents (with the column names from the
/// header) and `None` for object documents (key/value entries).
struct Structure<'a> {
    columns: Option<Vec<String>>,
    array_rows: Vec<Vec<&'a str>>,
    object_entries: Vec<(&'a str, &'a str)>,
}

/// A TOON array-of-objects header has the shape `[N]{c1,c2,...}:` (or
/// `name[N]{c1,...}:` for XML records): an opening brace, a matching closing
/// brace, and a trailing `:`. Data rows never contain braces, so this cleanly
/// distinguishes the header from body lines.
fn is_header_line(line: &str) -> bool {
    let t = line.trim_start();
    match (t.find('{'), t.rfind('}')) {
        (Some(ob), Some(cb)) if ob < cb => t[cb..].starts_with(':'),
        _ => false,
    }
}

fn scan_structure(toon: &str) -> (Option<Vec<String>>, Vec<Vec<&str>>, Vec<(&str, &str)>) {
    let lines: Vec<&str> = toon.lines().collect();
    let mut header_idx: Option<usize> = None;
    for (i, l) in lines.iter().enumerate() {
        if is_header_line(l) {
            header_idx = Some(i);
            break;
        }
    }

    if let Some(hi) = header_idx {
        let header = lines[hi];
        let columns = parse_columns(header);
        let mut rows: Vec<Vec<&str>> = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            if i == hi {
                continue;
            }
            if l.trim().is_empty() {
                continue;
            }
            rows.push(split_cells(l.trim_start()));
        }
        return (Some(columns), rows, Vec::new());
    }

    // Object document: `key: value` lines.
    let mut entries: Vec<(&str, &str)> = Vec::new();
    for l in &lines {
        if l.trim().is_empty() {
            continue;
        }
        if let Some(colon) = l.find(": ") {
            let key = l[..colon].trim();
            let val = l[colon + 2..].trim();
            entries.push((key, val));
        }
    }
    (None, Vec::new(), entries)
}

fn parse_columns(header: &str) -> Vec<String> {
    let open = match header.find('{') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let close = match header[open..].find('}') {
        Some(j) => open + j,
        None => return Vec::new(),
    };
    header[open + 1..close].split(',').map(|c| c.trim().to_string()).collect()
}

/// Splits a single TOON row/line into cells, respecting quoted spans
/// (strings containing commas are quoted in TOON).
fn split_cells(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut cells = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let mut in_quote = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            if in_quote && i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                i += 2;
                continue;
            }
            in_quote = !in_quote;
        } else if b == b',' && !in_quote {
            cells.push(&line[start..i]);
            start = i + 1;
        }
        i += 1;
    }
    cells.push(&line[start..]);
    cells
}

fn transform_cells<'a>(
    toon: &'a str,
    columns: &Option<Vec<String>>,
    array_rows: &[Vec<&'a str>],
    object_entries: &[(&'a str, &'a str)],
    abbrev: &HashMap<&'a str, String>,
    is_protected: &impl Fn(&str) -> bool,
) -> String {
    let lines: Vec<&str> = toon.lines().collect();
    let mut out = String::new();

    if let Some(cols) = columns {
        for l in &lines {
            let trimmed = l.trim_start();
            if trimmed.contains("]:") && trimmed.contains('{') && trimmed.contains('}') {
                out.push_str(l);
                out.push('\n');
                continue;
            }
            if l.trim().is_empty() {
                out.push('\n');
                continue;
            }
            let indent = l.len() - l.trim_start().len();
            let indent_str = &l[..indent];
            let cells = split_cells(trimmed);
            let mut rebuilt = String::new();
            for (ci, cell) in cells.iter().enumerate() {
                if ci > 0 {
                    rebuilt.push(',');
                }
                let protected = cols.get(ci).map(|c| is_protected(c.as_str())).unwrap_or(false);
                if !protected {
                    if let Some(sentinel) = abbrev.get(cell) {
                        rebuilt.push_str(sentinel);
                        continue;
                    }
                }
                rebuilt.push_str(cell);
            }
            out.push_str(indent_str);
            out.push_str(&rebuilt);
            out.push('\n');
        }
    } else {
        for l in &lines {
            if l.trim().is_empty() {
                out.push('\n');
                continue;
            }
            if let Some(colon) = l.find(": ") {
                let key = l[..colon].trim();
                let rest = &l[colon + 2..];
                if is_protected(key) {
                    out.push_str(l);
                } else if let Some(sentinel) = abbrev.get(rest.trim()) {
                    out.push_str(&l[..colon + 2]);
                    out.push_str(sentinel);
                } else {
                    out.push_str(l);
                }
            } else {
                out.push_str(l);
            }
            out.push('\n');
        }
    }

    // Keep the compiler honest about unused inputs in the object branch.
    let _ = (array_rows, object_entries);
    out
}

fn replace_cells_expand(line: &str, map: &HashMap<&str, &str>) -> String {
    if map.is_empty() {
        return line.to_string();
    }
    let cells = split_cells(line);
    let mut rebuilt = String::new();
    for (ci, cell) in cells.iter().enumerate() {
        if ci > 0 {
            rebuilt.push(',');
        }
        if let Some(original) = map.get(cell) {
            rebuilt.push_str(original);
        } else {
            rebuilt.push_str(cell);
        }
    }
    rebuilt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode_toon;

    #[test]
    fn round_trips_array_with_repeated_values() {
        let v = serde_json::json!([
            {"id": 1, "name": "Alice", "status": "administrator"},
            {"id": 2, "name": "Bob", "status": "administrator"},
            {"id": 3, "name": "Cara", "status": "administrator"},
            {"id": 4, "name": "Dana", "status": "administrator"},
            {"id": 5, "name": "Eli", "status": "administrator"},
        ]);
        let toon = encode_toon(&v).expect("encode");
        eprintln!("TOON=\n{toon}\n---");
        let dict = apply_dict(&toon, &[]);
        eprintln!("DICT={dict:?}");
        let dict = dict.expect("should compress (repeated 'administrator')");
        assert!(dict.starts_with(LEGEND_MARKER));
        assert_eq!(expand_legend(&dict), toon, "legend expansion must be lossless");
    }

    #[test]
    fn round_trips_object_document() {
        let v = serde_json::json!({"a": 1, "x": "pending_review_status", "y": "pending_review_status", "z": "pending_review_status"});
        let toon = encode_toon(&v).expect("encode");
        let dict = apply_dict(&toon, &[]).expect("should compress (3 repeated values)");
        assert_eq!(expand_legend(&dict), toon);
    }

    #[test]
    fn protects_critical_columns() {
        let v = serde_json::json!([
            {"id": 1, "role": "administrator"},
            {"id": 2, "role": "administrator"},
            {"id": 3, "role": "administrator"},
            {"id": 4, "role": "administrator"},
            {"id": 5, "role": "administrator"},
        ]);
        let toon = encode_toon(&v).expect("encode");
        assert!(
            apply_dict(&toon, &["role".to_string()]).is_none(),
            "protected column must not be compressed"
        );
        assert!(apply_dict(&toon, &[]).is_some(), "without protection it compresses");
    }

    #[test]
    fn no_benefit_returns_none() {
        let v = serde_json::json!([
            {"id": 1, "name": "Alice Chen"},
            {"id": 2, "name": "Bob Diaz"},
        ]);
        let toon = encode_toon(&v).expect("encode");
        assert!(apply_dict(&toon, &[]).is_none());
    }

    #[test]
    fn expand_without_legend_is_identity() {
        let v = serde_json::json!([
            {"id": 1, "name": "Alice Chen"},
            {"id": 2, "name": "Bob Diaz"},
        ]);
        let toon = encode_toon(&v).expect("encode");
        assert_eq!(expand_legend(&toon), toon);
    }
}
