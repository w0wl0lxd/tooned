// SPDX-License-Identifier: AGPL-3.0-only

//! Shared public types for the tooned workspace.

/// Supported source document types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocType {
    Json,
    NdJson,
    Yaml,
    Toml,
    Csv,
    Tsv,
    Xml,
}

/// Tunables for conversion functions. See [`ConversionOptions::default`]
/// for the constitution-mandated defaults (2% margin, 2 MiB cap).
#[derive(Debug, Clone, PartialEq)]
pub struct ConversionOptions {
    /// Minimum percentage by which TOON must beat compact JSON before it is
    /// surfaced as a conversion (constitution Principle II). Default: 2.0.
    pub margin_pct: f64,
    /// Hard cap on `input.len()`; inputs above this short-circuit to
    /// `Passthrough { reason: InputTooLarge }` before any parsing is
    /// attempted (constitution Technology Constraints). Default: 2 MiB.
    pub max_input_bytes: usize,
    /// When set, honored unconditionally over content-based sniffing (even
    /// if it conflicts with the actual content).
    pub format_hint: Option<DocType>,
    /// Opt-in precise BPE-token-based savings estimate. MUST NOT run on the
    /// default hot path (constitution Principle II); implemented in the
    /// Polish phase (T076). Default: false.
    pub precise_tokens: bool,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            margin_pct: 2.0,
            max_input_bytes: 2 * 1024 * 1024,
            format_hint: None,
            precise_tokens: false,
        }
    }
}

/// Why conversion did not surface a `Toon` conversion (or, when it did,
/// the internal decision path an equivalent `PassthroughReason` would have
/// taken).
#[derive(Debug, Clone, PartialEq)]
pub enum PassthroughReason {
    /// `detect` could not sniff a supported doctype from the content, and no
    /// `format_hint` was given.
    NotStructuredData,
    /// A doctype was detected/hinted, but parsing into a structured value
    /// failed.
    ParseFailed,
    /// `input.len() > max_input_bytes`; no parser was invoked.
    InputTooLarge,
    /// TOON encoded smaller than compact JSON, but not by more than
    /// `margin_pct` (constitution Principle II).
    NotSmallerEnough { json_bytes: usize, toon_bytes: usize },
    /// The conversion was computed and beat the margin, but decoding it back
    /// did not reproduce the original value (FR-008); never surfaced as
    /// `Toon`.
    RoundTripMismatch,
}

/// Reserved for genuine caller misuse or explicit decode failures -- never
/// returned by conversion functions for payload-driven failure
/// (malformed/oversized/ambiguous input), which always resolves to
/// `Conversion::Passthrough` instead (constitution Principle I).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ToonedError {
    /// Input exceeded a caller-declared size limit. Not actually returned by
    /// conversion functions (those downgrade to
    /// `Passthrough { reason: InputTooLarge }` instead, per FR-006); kept as
    /// a variant for callers that want to pre-validate a size limit
    /// themselves before calling in.
    #[error("input exceeds max_input_bytes limit")]
    InputTooLarge,
    /// `decode_toon` failed because `text` is not valid TOON.
    #[error("failed to decode TOON input: {0}")]
    DecodeFailed(String),
}

/// Dry-run diagnostic report (contract: never carries TOON text).
#[derive(Debug, Clone, PartialEq)]
pub struct InspectReport {
    pub doc_type: Option<DocType>,
    pub shape: ShapeClass,
    pub input_bytes: usize,
    pub json_bytes: Option<usize>,
    pub toon_bytes: Option<usize>,
    pub savings_pct: Option<f64>,
    /// Opt-in BPE-token-based savings estimate (T076, FR-023). `None`
    /// unless `ConversionOptions.precise_tokens` was `true` AND a
    /// conversion was actually computed (i.e. the same conditions under
    /// which `toon_bytes`/`savings_pct` are `Some`).
    pub precise_savings_pct: Option<f64>,
    pub would_convert: bool,
    pub reason: Option<PassthroughReason>,
}

/// Result of an adaptive conversion decision (data-model.md).
#[derive(Debug, Clone, PartialEq)]
pub enum Conversion {
    Toon { text: String, report: ConversionReport },
    Passthrough { bytes: Vec<u8>, reason: PassthroughReason },
}

/// Diagnostic detail attached to a successful `Conversion::Toon`
/// (data-model.md).
#[derive(Debug, Clone, PartialEq)]
pub struct ConversionReport {
    pub doc_type: DocType,
    pub shape: ShapeClass,
    pub json_bytes: usize,
    pub toon_bytes: usize,
    pub savings_pct: f64,
}

/// Payload shape classification: `K = 64` sampling,
/// per-element key-signature, `uniformity_pct` computation.
///
/// Descriptive/diagnostic only -- per data-model, this does
/// NOT gate the conversion decision on its own; the byte-size comparison is
/// the sole gate.
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeClass {
    UniformArrayOfObjects { uniformity_pct: f64, sampled: usize },
    Irregular,
    Scalar,
}
