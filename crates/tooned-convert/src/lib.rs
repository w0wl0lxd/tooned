// SPDX-License-Identifier: AGPL-3.0-only

//! Conversion orchestration and shape classification.
//!
//! Both public functions are thin wrappers over a single shared pipeline
//! (`attempt`) -- constitution Principle V ("no parallel implementation"):
//! detect -> parse -> shape-classify -> encode -> margin check -> round-trip
//! check. `maybe_tooned` surfaces the encoded TOON text on success;
//! `inspect` computes the same decision (so it can report accurate sizes and
//! a convertible y/n verdict) but never returns the TOON text itself.

use serde_json::Value;
use tooned_detect::detect;
use tooned_parse::ParseError;
use tooned_toon::encode_toon;
use tooned_types::{
    Conversion, ConversionOptions, ConversionReport, DocType, InspectReport, PassthroughReason,
    ShapeClass, ToonedError,
};

mod shape;

pub mod onto;
pub use onto::{decode as decode_onto, encode as encode_onto, maybe_onto};

pub mod tron;
pub use tron::{
    StreamStats, decode as decode_tron, encode as encode_tron, maybe_tron, maybe_tron_stream,
};

/// A successfully-encoded TOON candidate, kept internal to `attempt`'s
/// result -- only `maybe_tooned` ever surfaces the `text` field publicly.
struct AttemptToon {
    text: String,
    bytes: usize,
}

/// A `std::io::Write` sink that only tallies bytes written, never storing
/// them -- used to get `serde_json::to_writer`'s serialized byte length
/// without materializing an owned `String` (see `attempt`'s hot-path
/// comment).
pub(crate) struct ByteCountingWriter(usize);

impl std::io::Write for ByteCountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The outcome of running the full detect/parse/shape/encode/round-trip
/// pipeline once. `reason.is_none()` means "convertible" -- `toon` is
/// guaranteed `Some` in that case (see `attempt`'s postcondition, enforced
/// defensively rather than assumed at every call site).
struct Attempt {
    doc_type: Option<DocType>,
    shape: ShapeClass,
    json_bytes: Option<usize>,
    /// The compact-JSON text itself, kept only for the opt-in precise-token
    /// estimate (T076) -- never surfaced on `maybe_tooned`'s hot path.
    json_text: Option<String>,
    toon: Option<AttemptToon>,
    reason: Option<PassthroughReason>,
}

impl Attempt {
    fn not_structured() -> Self {
        Attempt {
            doc_type: None,
            shape: ShapeClass::Scalar,
            json_bytes: None,
            json_text: None,
            toon: None,
            reason: Some(PassthroughReason::NotStructuredData),
        }
    }

    fn parse_failed(doc_type: DocType) -> Self {
        Attempt {
            doc_type: Some(doc_type),
            shape: ShapeClass::Scalar,
            json_bytes: None,
            json_text: None,
            toon: None,
            reason: Some(PassthroughReason::ParseFailed),
        }
    }
}

/// Internal dispatcher that calls the appropriate format parser.
pub(crate) fn parse_by_doc_type(input: &[u8], doc_type: DocType) -> Result<Value, ParseError> {
    match doc_type {
        DocType::Json => tooned_json::parse_json(input),
        DocType::NdJson => tooned_json::parse_ndjson(input),
        DocType::Yaml => tooned_yaml::parse_yaml(input),
        DocType::Toml => tooned_toml::parse_toml(input),
        DocType::Csv => tooned_csv::parse_csv(input),
        DocType::Tsv => tooned_csv::parse_tsv(input),
        DocType::Xml => tooned_xml::parse(input),
    }
}

/// Runs the full pipeline once. Never panics: every fallible step folds
/// into a `PassthroughReason` rather than propagating a panic or an `Err`
/// (constitution Principle I; `maybe_tooned`/`inspect` never `Err` for
/// payload-driven failure).
fn attempt(input: &[u8], opts: &ConversionOptions) -> Attempt {
    let Some(doc_type) = detect(input, opts.format_hint) else {
        return Attempt::not_structured();
    };

    let Ok(value) = parse_by_doc_type(input, doc_type) else {
        return Attempt::parse_failed(doc_type);
    };

    let shape = shape::classify(&value);

    // `maybe_tooned`'s hot path (the common case, `opts.precise_tokens ==
    // false`) only ever needs `json_bytes`'s length, never the text itself
    // (`json_text: _` is discarded at the call site) -- so avoid the O(n)
    // heap allocation of a full owned `String` there, counting bytes via a
    // `Write` sink instead. Only when a caller actually opted into precise
    // BPE-token savings (`inspect`'s `precise_token_savings_pct`, the sole
    // consumer of `json_text`'s contents) is the owned `String` built.
    //
    // A value with no JSON representation at all (e.g. a NaN/Infinity float
    // smuggled in via YAML/TOML's more permissive float literals) -- fail
    // closed, not a panic.
    let (json_bytes, json_text) = if opts.precise_tokens {
        let Ok(text) = serde_json::to_string(&value) else {
            return Attempt {
                doc_type: Some(doc_type),
                shape,
                json_bytes: None,
                json_text: None,
                toon: None,
                reason: Some(PassthroughReason::ParseFailed),
            };
        };
        (text.len(), Some(text))
    } else {
        let mut counter = ByteCountingWriter(0);
        let Ok(()) = serde_json::to_writer(&mut counter, &value) else {
            return Attempt {
                doc_type: Some(doc_type),
                shape,
                json_bytes: None,
                json_text: None,
                toon: None,
                reason: Some(PassthroughReason::ParseFailed),
            };
        };
        (counter.0, None)
    };

    let Ok(encoded) = encode_toon(&value) else {
        return Attempt {
            doc_type: Some(doc_type),
            shape,
            json_bytes: Some(json_bytes),
            json_text,
            toon: None,
            reason: Some(PassthroughReason::ParseFailed),
        };
    };
    let toon_bytes = encoded.len();

    if !is_smaller_enough(json_bytes, toon_bytes, opts.margin_pct) {
        return Attempt {
            doc_type: Some(doc_type),
            shape,
            json_bytes: Some(json_bytes),
            json_text,
            toon: Some(AttemptToon { text: encoded, bytes: toon_bytes }),
            reason: Some(PassthroughReason::NotSmallerEnough { json_bytes, toon_bytes }),
        };
    }

    let round_trip_ok = match tooned_toon::decode_toon_with_limit(&encoded, opts.max_input_bytes) {
        Ok(decoded) => decoded == value,
        Err(_) => false,
    };

    if !round_trip_ok {
        return Attempt {
            doc_type: Some(doc_type),
            shape,
            json_bytes: Some(json_bytes),
            json_text,
            toon: Some(AttemptToon { text: encoded, bytes: toon_bytes }),
            reason: Some(PassthroughReason::RoundTripMismatch),
        };
    }

    Attempt {
        doc_type: Some(doc_type),
        shape,
        json_bytes: Some(json_bytes),
        json_text,
        toon: Some(AttemptToon { text: encoded, bytes: toon_bytes }),
        reason: None,
    }
}

/// Opt-in precise BPE-token-based savings estimate (T076,
/// `ConversionOptions.precise_tokens`). Uses `tiktoken-rs`'s `cl100k_base`
/// tokenizer -- constructed (and its bundled rank table parsed) lazily via
/// `tiktoken_rs::cl100k_base_singleton()` only the first time this function
/// is actually called, i.e. only when a caller opts in; the default hot
/// path (`maybe_tooned`) never calls this function at all (constitution
/// Principle II: "MUST NOT run on the default hot path").
fn precise_token_savings_pct(json_text: &str, toon_text: &str) -> f64 {
    let bpe = tiktoken_rs::cl100k_base_singleton();
    let json_tokens = bpe.encode_ordinary(json_text).len();
    let toon_tokens = bpe.encode_ordinary(toon_text).len();
    if json_tokens == 0 {
        return 0.0;
    }
    (1.0 - (toon_tokens as f64 / json_tokens as f64)) * 100.0
}

/// `toon_bytes < json_bytes * (1 - margin_pct / 100)` (data-model.md
/// validation rule). `margin_pct` is clamped to a finite, non-negative value
/// first: a caller-supplied NaN/negative/infinite margin must never produce
/// a NaN comparison (which is always `false` and would silently behave like
/// an infinite margin) -- clamping to 0 is the conservative, still-safe
/// interpretation ("no margin required" rather than "reject everything" or
/// "accept everything").
pub fn is_smaller_enough(json_bytes: usize, toon_bytes: usize, margin_pct: f64) -> bool {
    let margin_pct = if margin_pct.is_finite() { margin_pct.max(0.0) } else { 0.0 };
    let threshold = (json_bytes as f64) * (1.0 - margin_pct / 100.0);
    (toon_bytes as f64) < threshold
}

pub(crate) fn compute_savings_pct(json_bytes: usize, toon_bytes: usize) -> f64 {
    if json_bytes == 0 {
        return 0.0;
    }
    (1.0 - (toon_bytes as f64 / json_bytes as f64)) * 100.0
}

/// Never returns `Err` for payload-driven failure (malformed/oversized/
/// ambiguous input) -- those always resolve to
/// `Ok(Conversion::Passthrough { .. })`. `Err` is reserved for caller
/// misuse.
///
/// # Errors
/// Currently infallible in practice (no `ConversionOptions` value is
/// rejected outright; adversarial values are clamped defensively instead --
/// see `is_smaller_enough`); the `Result` return type is kept per the
/// contract to leave room for future caller-misuse validation without a
/// breaking signature change.
pub fn maybe_tooned(input: &[u8], opts: &ConversionOptions) -> Result<Conversion, ToonedError> {
    if input.len() > opts.max_input_bytes {
        return Ok(Conversion::Passthrough {
            bytes: input.to_vec(),
            reason: PassthroughReason::InputTooLarge,
        });
    }

    let Attempt { doc_type, shape, json_bytes, json_text: _, toon, reason } = attempt(input, opts);

    if let Some(reason) = reason {
        return Ok(Conversion::Passthrough { bytes: input.to_vec(), reason });
    }

    let (Some(doc_type), Some(json_bytes), Some(toon)) = (doc_type, json_bytes, toon) else {
        // Unreachable given `attempt`'s contract (reason.is_none() implies
        // all three are Some), but an internal-invariant slip must still
        // fail safe to Passthrough, never panic.
        return Ok(Conversion::Passthrough {
            bytes: input.to_vec(),
            reason: PassthroughReason::ParseFailed,
        });
    };

    Ok(Conversion::Toon {
        text: toon.text,
        report: ConversionReport {
            doc_type,
            shape,
            json_bytes,
            toon_bytes: toon.bytes,
            savings_pct: compute_savings_pct(json_bytes, toon.bytes),
        },
    })
}

/// Dry-run: same detection + shape classification as [`maybe_tooned`], but
/// the returned report never carries the TOON text itself -- only sizes and
/// a convertible y/n verdict (backs `tooned check`). Internally this still
/// has to encode (and decode, for the round-trip check) to measure sizes and
/// confirm fidelity accurately; the point is that the string is never part
/// of the public `InspectReport`, not that encoding is skipped.
pub fn inspect(input: &[u8], opts: &ConversionOptions) -> InspectReport {
    if input.len() > opts.max_input_bytes {
        return InspectReport {
            doc_type: None,
            shape: ShapeClass::Scalar,
            input_bytes: input.len(),
            json_bytes: None,
            toon_bytes: None,
            savings_pct: None,
            precise_savings_pct: None,
            would_convert: false,
            reason: Some(PassthroughReason::InputTooLarge),
        };
    }

    let Attempt { doc_type, shape, json_bytes, json_text, toon, reason } = attempt(input, opts);
    let toon_bytes = toon.as_ref().map(|t| t.bytes);
    let savings_pct = match (json_bytes, toon_bytes) {
        (Some(j), Some(t)) => Some(compute_savings_pct(j, t)),
        _ => None,
    };
    // T076: only ever computed when the caller opts in -- `precise_tokens`
    // defaults to `false`, and this branch (hence `tiktoken-rs`'s tokenizer
    // construction) is never reached otherwise.
    let precise_savings_pct = match (opts.precise_tokens, &json_text, &toon) {
        (true, Some(json_text), Some(toon)) => {
            Some(precise_token_savings_pct(json_text, &toon.text))
        }
        _ => None,
    };

    InspectReport {
        doc_type,
        shape,
        input_bytes: input.len(),
        json_bytes,
        toon_bytes,
        savings_pct,
        precise_savings_pct,
        would_convert: reason.is_none(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    /// `rows` near-identical objects sharing a key set -- realistic TOON
    /// savings (~40-60%), reliably below the size the max-input-bytes tests
    /// want to short-circuit past.
    fn build_uniform_array_payload(rows: usize) -> Vec<u8> {
        let mut s = String::from("[");
        for i in 0..rows {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(s, r#"{{"id":{i},"name":"row-{i}","active":true,"score":{i}.5}}"#);
        }
        s.push(']');
        s.into_bytes()
    }

    #[test]
    fn max_input_bytes_short_circuits_before_parsing() {
        // Well-formed, genuinely convertible JSON -- if the size gate didn't
        // fire first, this would produce Conversion::Toon (verified below
        // at a larger max_input_bytes), not Passthrough(InputTooLarge). The
        // tiny max_input_bytes proves the gate runs strictly before
        // detect/parse ever sees the bytes.
        let payload = build_uniform_array_payload(50);
        let opts = ConversionOptions { max_input_bytes: 4, ..ConversionOptions::default() };
        let result = maybe_tooned(&payload, &opts).expect("infallible for payload-driven input");
        match result {
            Conversion::Passthrough { reason: PassthroughReason::InputTooLarge, bytes } => {
                assert_eq!(bytes, payload);
            }
            other => panic!("expected Passthrough(InputTooLarge), got {other:?}"),
        }
    }

    #[test]
    fn same_payload_converts_with_a_generous_max_input_bytes() {
        // Companion to the test above: proves the fixture really is
        // convertible when the size gate doesn't intervene.
        let payload = build_uniform_array_payload(50);
        let opts = ConversionOptions::default();
        let result = maybe_tooned(&payload, &opts).expect("infallible for payload-driven input");
        assert!(matches!(result, Conversion::Toon { .. }));
    }

    #[test]
    fn savings_below_margin_downgrades_to_passthrough() {
        let payload = build_uniform_array_payload(10);

        let baseline_opts = ConversionOptions { margin_pct: 0.0, ..ConversionOptions::default() };
        let baseline =
            maybe_tooned(&payload, &baseline_opts).expect("infallible for payload-driven input");
        let Conversion::Toon { report, .. } = baseline else {
            panic!(
                "fixture payload must be convertible at margin=0 for this test to be meaningful"
            );
        };
        assert!(report.savings_pct > 0.0);

        // A margin strictly greater than the payload's real savings must
        // downgrade the same payload to Passthrough(NotSmallerEnough).
        let opts = ConversionOptions {
            margin_pct: report.savings_pct + 5.0,
            ..ConversionOptions::default()
        };
        let result = maybe_tooned(&payload, &opts).expect("infallible for payload-driven input");
        match result {
            Conversion::Passthrough {
                reason: PassthroughReason::NotSmallerEnough { json_bytes, toon_bytes },
                ..
            } => {
                assert_eq!(json_bytes, report.json_bytes);
                assert_eq!(toon_bytes, report.toon_bytes);
                assert!(toon_bytes < json_bytes, "TOON must still be smaller, just not enough");
            }
            other => panic!("expected Passthrough(NotSmallerEnough), got {other:?}"),
        }
    }

    #[test]
    fn round_trip_mismatch_downgrades_to_passthrough() {
        // A genuine TOON codec edge case (not a mock/seam): a whole-number
        // JSON float like `1.0` round-trips through TOON as the integer `1`
        // (`Number(1.0) != Number(1)` under serde_json::Value's own
        // equality), so this MUST downgrade to Passthrough rather than
        // silently surfacing a corrupted conversion (FR-008, constitution
        // Principle I).
        let payload: &[u8] = br#"{"x": 1.0, "y": 2.0, "z": 3.0, "note": "whole-number floats"}"#;
        let opts = ConversionOptions { margin_pct: 0.0, ..ConversionOptions::default() };
        let result = maybe_tooned(payload, &opts).expect("infallible for payload-driven input");
        match result {
            Conversion::Passthrough { reason: PassthroughReason::RoundTripMismatch, bytes } => {
                assert_eq!(bytes, payload);
            }
            other => panic!("expected Passthrough(RoundTripMismatch), got {other:?}"),
        }
    }

    #[test]
    fn non_structured_input_passes_through() {
        let payload = b"just some prose, nothing structured here";
        let opts = ConversionOptions::default();
        let result = maybe_tooned(payload, &opts).expect("infallible for payload-driven input");
        match result {
            Conversion::Passthrough { reason: PassthroughReason::NotStructuredData, bytes } => {
                assert_eq!(bytes, payload);
            }
            other => panic!("expected Passthrough(NotStructuredData), got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_passes_through() {
        let payload = b"{\"a\": not valid";
        let opts = ConversionOptions::default();
        let result = maybe_tooned(payload, &opts).expect("infallible for payload-driven input");
        match result {
            Conversion::Passthrough { reason: PassthroughReason::ParseFailed, bytes } => {
                assert_eq!(bytes, payload);
            }
            other => panic!("expected Passthrough(ParseFailed), got {other:?}"),
        }
    }

    #[test]
    fn inspect_never_returns_toon_text_but_reports_sizes() {
        let payload = build_uniform_array_payload(10);
        let opts = ConversionOptions::default();
        let report = inspect(&payload, &opts);
        assert!(report.would_convert);
        assert!(report.json_bytes.is_some());
        assert!(report.toon_bytes.is_some());
        assert!(report.savings_pct.is_some());
        assert_eq!(report.reason, None);
    }

    #[test]
    fn inspect_computes_precise_token_savings_only_when_opted_in() {
        // T076: `precise_tokens` is opt-in (default false) and MUST NOT run
        // on the default hot path (constitution Principle II). `inspect`
        // (not `maybe_tooned`) is the only entrypoint that ever computes
        // this -- it backs `tooned check --precise`.
        let payload = build_uniform_array_payload(30);

        let default_opts = ConversionOptions::default();
        assert!(!default_opts.precise_tokens);
        let report = inspect(&payload, &default_opts);
        assert_eq!(
            report.precise_savings_pct, None,
            "precise_tokens defaults to false -- no BPE tokenization estimate must be present"
        );

        let precise_opts =
            ConversionOptions { precise_tokens: true, ..ConversionOptions::default() };
        let report = inspect(&payload, &precise_opts);
        let precise_savings_pct =
            report.precise_savings_pct.expect("precise_tokens: true must compute an estimate");
        assert!(
            precise_savings_pct > 0.0,
            "this fixture is genuinely convertible, so the BPE-token estimate should also show savings"
        );
    }

    #[test]
    fn inspect_reports_input_too_large_without_parsing() {
        let payload = build_uniform_array_payload(50);
        let opts = ConversionOptions { max_input_bytes: 4, ..ConversionOptions::default() };
        let report = inspect(&payload, &opts);
        assert!(!report.would_convert);
        assert_eq!(report.reason, Some(PassthroughReason::InputTooLarge));
        assert_eq!(report.input_bytes, payload.len());
        assert_eq!(report.doc_type, None);
    }
}
