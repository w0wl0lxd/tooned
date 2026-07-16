// SPDX-License-Identifier: AGPL-3.0-only

//! Format sniffing (JSON/NDJSON/YAML/TOML/CSV/TSV), hint-first.
//!
//! Operates purely on raw bytes -- never requires valid UTF-8 -- so it can
//! run safely before any encoding validation. This is deliberate:
//! `detect`/`sniff` must never be the thing that panics on adversarial
//! input (constitution Principle I), and byte-level line/prefix inspection
//! sidesteps UTF-8 concerns entirely rather than needing to handle them.

use tooned_types::DocType;

#[cfg(test)]
use std::alloc::System;

#[cfg(test)]
#[global_allocator]
pub(crate) static GLOBAL: heapster::Heapster<System> = heapster::Heapster::new(System);

/// Detects the document type of `input`. `format_hint`, when present, is
/// honored unconditionally -- even if it conflicts with the content -- per
/// FR-002's explicit-hint-first contract.
pub fn detect(input: &[u8], format_hint: Option<DocType>) -> Option<DocType> {
    if let Some(hint) = format_hint {
        return Some(hint);
    }
    sniff(input)
}

fn lines(input: &[u8]) -> impl Iterator<Item = &[u8]> {
    input.split(|b| *b == b'\n')
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// A plain iterate-and-count over what are always short (single-line)
/// slices here -- not remotely hot enough to justify pulling in the
/// `bytecount` crate as a dependency (clippy's `naive_bytecount` lint is
/// tuned for large-buffer scanning, which this isn't).
#[allow(clippy::naive_bytecount)]
fn count_byte(haystack: &[u8], needle: u8) -> usize {
    haystack.iter().filter(|b| **b == needle).count()
}

fn sniff(input: &[u8]) -> Option<DocType> {
    let trimmed = input.trim_ascii();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.first() == Some(&b'{') {
        if is_ndjson(input) {
            return Some(DocType::NdJson);
        }
        return Some(DocType::Json);
    }

    if trimmed.first() == Some(&b'[') {
        // `[` is ambiguous: a JSON array literal and a TOML table header
        // (`[section]`/`[[array-of-tables]]`) both start with it. Check the
        // TOML-header shape first -- a bare identifier filling the whole
        // bracket pair on one line, no JSON punctuation inside -- since a
        // real JSON array's first line always contains something a TOML
        // header line never does (a value, a comma, or nothing at all).
        if looks_like_toml_header_line(trimmed) {
            return Some(DocType::Toml);
        }
        if is_ndjson(input) {
            return Some(DocType::NdJson);
        }
        return Some(DocType::Json);
    }

    if trimmed.starts_with(b"---") {
        return Some(DocType::Yaml);
    }

    // TOML (`key = value`) and YAML (`key: value`) heuristics run *before*
    // delimiter sniffing: both check only the first meaningful line and are
    // unambiguous about `=`/`: ` syntax, whereas `sniff_delimited` merely
    // counts commas/tabs and has no such awareness. Checking delimiters
    // first would let legitimate TOML/YAML content whose lines happen to
    // contain matching comma counts (e.g. quoted string values with commas,
    // or consecutive TOML array lines) be silently misdetected as CSV/TSV
    // and then garbage-parsed instead of correctly parsed or passed through.
    if is_toml(input) {
        return Some(DocType::Toml);
    }

    if is_yaml(input) {
        return Some(DocType::Yaml);
    }

    if let Some(delimited) = sniff_delimited(input) {
        return Some(delimited);
    }

    tooned_xml::sniff(input)
}

/// Whether the first line of `trimmed` is, by itself, a TOML table header:
/// one or two bracket pairs wrapping a bare dotted identifier and nothing
/// else (`[section]`, `[[array.of.tables]]`) -- never true for a JSON array
/// literal, which always contains a value, a comma, or is empty (`[]`).
///
/// The identifier check alone isn't quite enough: a single-element JSON
/// array whose element is a bare literal -- `[null]`, `[true]`, `[42]` --
/// is *also* just an identifier-shaped bracket pair, and would otherwise be
/// misdetected as TOML (silently corrupting the value: `[null]` parsed as
/// TOML means "an empty table named `null`", not "an array containing
/// null"). Excluding anything that's itself a valid JSON scalar literal
/// resolves that collision in JSON's favor, which is the only concrete case
/// this heuristic can actually get wrong -- real TOML section names are
/// essentially never exactly `true`/`false`/`null`/a bare number.
fn looks_like_toml_header_line(trimmed: &[u8]) -> bool {
    let first_line = match trimmed.split(|b| *b == b'\n').next() {
        Some(line) => line.trim_ascii(),
        None => return false,
    };
    match strip_toml_brackets(first_line) {
        Some(inner) => {
            !inner.is_empty()
                && !looks_like_json_scalar_literal(inner)
                && inner
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        }
        None => false,
    }
}

fn looks_like_json_scalar_literal(bytes: &[u8]) -> bool {
    matches!(bytes, b"true" | b"false" | b"null") || looks_like_numeric_literal(bytes)
}

fn looks_like_numeric_literal(bytes: &[u8]) -> bool {
    let digits = match bytes.strip_prefix(b"-") {
        Some(rest) => rest,
        None => bytes,
    };
    if digits.is_empty() {
        return false;
    }
    let mut seen_digit = false;
    let mut seen_dot = false;
    for &b in digits {
        if b.is_ascii_digit() {
            seen_digit = true;
        } else if b == b'.' && !seen_dot {
            seen_dot = true;
        } else {
            return false;
        }
    }
    seen_digit
}

/// Strips one or two matching `[...]` bracket pairs from `line`, returning
/// the innermost content. Returns `None` if `line` isn't bracket-wrapped at
/// all.
fn strip_toml_brackets(line: &[u8]) -> Option<&[u8]> {
    let mut current = line;
    let mut stripped_any = false;
    for _ in 0..2 {
        if current.len() < 2 || current.first() != Some(&b'[') || current.last() != Some(&b']') {
            break;
        }
        current = current.get(1..current.len() - 1)?;
        stripped_any = true;
    }
    if stripped_any { Some(current) } else { None }
}

/// NDJSON: at least 2 non-empty lines (sampling up to 3), each of which
/// looks like a complete, self-contained JSON object/array on its own line.
/// A pretty-printed single JSON document's inner lines don't have this
/// shape (they're partial, e.g. `  "key": value,`), which is what
/// distinguishes NDJSON from multi-line JSON here.
fn is_ndjson(input: &[u8]) -> bool {
    let mut sampled = 0usize;
    let mut json_like = 0usize;
    for line in lines(input) {
        let t = line.trim_ascii();
        if t.is_empty() {
            continue;
        }
        sampled += 1;
        let looks_json = (t.first() == Some(&b'{') && t.last() == Some(&b'}'))
            || (t.first() == Some(&b'[') && t.last() == Some(&b']'));
        if looks_json {
            json_like += 1;
        }
        if sampled >= 3 {
            break;
        }
    }
    sampled >= 2 && json_like == sampled
}

/// CSV/TSV: the first non-empty line's delimiter counts, cross-checked
/// against the second non-empty line for consistency. Requires at least two
/// content lines (a header plus one data row) -- a single line containing a
/// stray comma or tab (e.g. ordinary prose) is not strong enough evidence
/// on its own.
fn sniff_delimited(input: &[u8]) -> Option<DocType> {
    let mut content_lines = lines(input).map(<[u8]>::trim_ascii).filter(|l| !l.is_empty());
    let first = content_lines.next()?;
    let second = content_lines.next()?;

    let tab_count = count_byte(first, b'\t');
    let comma_count = count_byte(first, b',');

    if tab_count > 0 && tab_count >= comma_count {
        let consistent = count_byte(second, b'\t') == tab_count;
        if consistent {
            return Some(DocType::Tsv);
        }
        return None;
    }

    if comma_count > 0 {
        let consistent = count_byte(second, b',') == comma_count;
        if consistent {
            return Some(DocType::Csv);
        }
    }

    None
}

/// TOML: the first meaningful (non-blank, non-comment) line is a `[section]`
/// / `[[array-of-tables]]` header, or a `key = value` assignment that is not
/// also a YAML-style `key: value` mapping.
fn is_toml(input: &[u8]) -> bool {
    for line in lines(input) {
        let t = line.trim_ascii();
        if t.is_empty() || t.first() == Some(&b'#') {
            continue;
        }
        if t.first() == Some(&b'[') && t.last() == Some(&b']') {
            return true;
        }
        return contains_bytes(t, b" = ") && !contains_bytes(t, b": ");
    }
    false
}

/// YAML: the first meaningful line starts a sequence item (`- `) or looks
/// like a mapping entry (`key: value` or a bare `key:`).
fn is_yaml(input: &[u8]) -> bool {
    for line in lines(input) {
        let t = line.trim_ascii();
        if t.is_empty() || t.first() == Some(&b'#') {
            continue;
        }
        if t.first() == Some(&b'-') {
            return true;
        }
        return contains_bytes(t, b": ") || t.last() == Some(&b':');
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_hint_overrides_conflicting_content() {
        let json_bytes = b"{\"a\":1}";
        assert_eq!(detect(json_bytes, Some(DocType::Toml)), Some(DocType::Toml));
    }

    #[test]
    fn explicit_hint_overrides_even_garbage_content() {
        assert_eq!(detect(b"@@@not anything@@@", Some(DocType::Csv)), Some(DocType::Csv));
    }

    #[test]
    fn sniffs_json() {
        assert_eq!(detect(br#"{"a": 1, "b": [1,2,3]}"#, None), Some(DocType::Json));
        assert_eq!(detect(b"[1, 2, 3]", None), Some(DocType::Json));
        assert_eq!(
            detect(b"{\n  \"a\": 1,\n  \"b\": 2\n}\n", None),
            Some(DocType::Json),
            "pretty-printed single JSON document must not be mistaken for NDJSON"
        );
    }

    #[test]
    fn sniffs_ndjson() {
        let input = b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n";
        assert_eq!(detect(input, None), Some(DocType::NdJson));
    }

    #[test]
    fn sniffs_yaml() {
        let input = b"---\nname: alice\nage: 30\n";
        assert_eq!(detect(input, None), Some(DocType::Yaml));

        let input2 = b"name: alice\nage: 30\ntags:\n  - a\n  - b\n";
        assert_eq!(detect(input2, None), Some(DocType::Yaml));
    }

    #[test]
    fn sniffs_toml() {
        let input = b"[server]\nhost = \"localhost\"\nport = 8080\n";
        assert_eq!(detect(input, None), Some(DocType::Toml));

        let input2 = b"name = \"tooned\"\nversion = \"0.1.0\"\n";
        assert_eq!(detect(input2, None), Some(DocType::Toml));
    }

    #[test]
    fn single_element_scalar_json_arrays_are_not_mistaken_for_toml_headers() {
        // Regression test: `[null]`/`[true]`/`[42]` are identifier-shaped
        // bracket pairs, colliding with the TOML table-header heuristic
        // (`[null]` as TOML means "an empty table named null", silently
        // corrupting the value if misdetected -- caught by the T007
        // round-trip proptest).
        assert_eq!(detect(b"[null]", None), Some(DocType::Json));
        assert_eq!(detect(b"[true]", None), Some(DocType::Json));
        assert_eq!(detect(b"[false]", None), Some(DocType::Json));
        assert_eq!(detect(b"[42]", None), Some(DocType::Json));
        assert_eq!(detect(b"[-5]", None), Some(DocType::Json));
        assert_eq!(detect(b"[3.14]", None), Some(DocType::Json));
        // A genuine TOML section header must still be detected as such.
        assert_eq!(detect(b"[server]\nhost = 1\n", None), Some(DocType::Toml));
    }

    #[test]
    fn sniffs_csv() {
        let input = b"name,age,active\nalice,30,true\nbob,25,false\n";
        assert_eq!(detect(input, None), Some(DocType::Csv));
    }

    #[test]
    fn sniffs_tsv() {
        let input = b"name\tage\tactive\nalice\t30\ttrue\nbob\t25\tfalse\n";
        assert_eq!(detect(input, None), Some(DocType::Tsv));
    }

    #[test]
    fn unrecognized_content_returns_none() {
        assert_eq!(detect(b"", None), None);
        assert_eq!(detect(b"   \n\t  ", None), None);
        assert_eq!(detect(b"this is just some prose without any structure at all", None), None);
    }

    #[test]
    fn detect_is_zero_allocation_on_representative_inputs() {
        // `heapster` wraps the global allocator for this test binary. The
        // sniffing hot path must not allocate on the heap: it works purely
        // with byte slices and iterators, so any regression that introduces
        // a `Vec`, `String`, or boxed state in `detect`/`sniff` will fail
        // this guardrail.
        //
        // Skip under `cargo llvm-cov`: the coverage instrumentation itself
        // performs heap allocations and would false-positive this test.
        if std::env::var_os("CARGO_LLVM_COV").is_some() {
            return;
        }

        let cases: &[&[u8]] = &[
            br#"{"a": 1, "b": [1,2,3]}"#,
            b"[1, 2, 3]",
            b"{\"a\":1}\n{\"a\":2}\n",
            b"---\nname: alice\nage: 30\n",
            b"[server]\nhost = \"localhost\"\nport = 8080\n",
            b"name,age,active\nalice,30,true\nbob,25,false\n",
            b"name\tage\tactive\nalice\t30\ttrue\nbob\t25\tfalse\n",
            b"this is just some prose without any structure at all",
            b"",
        ];
        for input in cases {
            let (_, diff) = GLOBAL.measure(|| detect(input, None));
            assert_eq!(
                diff.alloc_count, 0,
                "detect({input:?}) must not perform any heap allocations"
            );
            assert_eq!(diff.alloc_sum, 0, "detect({input:?}) must not allocate any heap bytes");
        }
    }
}
