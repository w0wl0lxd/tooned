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
    /// MessagePack (binary, JSON-shaped documents and arrays).
    Msgpack,
    /// CBOR (Concise Binary Object Representation; binary JSON).
    Cbor,
    /// JSON5 (human-readable JSON superset with comments, trailing commas,
    /// unquoted keys, and single-quoted strings).
    Json5,
}

/// A tokenization profile used to measure *real* (model-aware) token savings
/// instead of the default 4-bytes-per-token heuristic. This is a plain,
/// dependency-free descriptor kept in `tooned-types` so the core crate stays
/// dependency-minimal (constitution Principle III); the actual BPE counting
/// lives in the `tooned-token` crate, which maps these profiles onto bundled
/// `tiktoken-rs` rank tables (cl100k / o200k) with no network calls.
///
/// Grounded in July-2026 research: arxiv 2607.15232 ("In-Place Tokenizer
/// Expansion") shows tokenization cost is a first-class budget axis, and the
/// "Structure-Aware Tokenization for JSON" line of work (2026) shows JSON
/// grammar is highly compressible under a schema-aware tokenizer — so the
/// *profile* a payload is measured against materially changes the reported
/// savings. (F1/F2.)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TokenizerProfile {
    /// The default 4-bytes/token rule of thumb (no BPE table).
    Heuristic,
    /// OpenAI `cl100k_base` (GPT-3.5/4 era models).
    Cl100k,
    /// OpenAI `o200k_base` (GPT-4o / o-series models).
    O200k,
    /// A named model; resolved to a bundled BPE by `tooned-token` (unknown
    /// names fall back to the heuristic). Serialized so it can ride along in
    /// config files and MCP requests.
    Named(String),
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
    /// Density-aware margin: when true, `margin_pct` is treated as a *floor*
    /// and the effective margin is widened for dense/high-redundancy payloads
    /// so that the adaptive converter does not spend the round-trip + dict
    /// legend overhead on inputs that barely beat the fixed margin. Honored
    /// only when strict byte-size comparison still decides the conversion;
    /// never weakens the lossless gate. Default: false (library); CLI turns
    /// it on.
    pub auto_margin: bool,
    /// Dictionary compression tier (#1). When true and a net-positive token
    /// dictionary can be extracted from the TOON text, the surfaced TOON is
    /// wrapped in a `legend:` block. Strictly net-win gated: applied only when
    /// it strictly shrinks the total bytes AND the resulting text still
    /// round-trips. Default: true.
    pub dict_enabled: bool,
    /// Policy controlling which columns/keys are protected from dictionary
    /// abbreviation and density-aware margin tuning (#3). Default:
    /// [`CriticalFieldPolicy::default_policy`].
    pub critical_policy: CriticalFieldPolicy,
    /// Tokenization profile used to measure real token savings. `None` keeps
    /// the constitution-mandated 4-bytes/token heuristic on the hot path
    /// (Principle II: MUST NOT run a BPE tokenizer by default). When `Some`,
    /// `precise_tokens` is implied and the reported `savings_pct` /
    /// `precise_savings_pct` reflect the chosen model's actual tokenizer.
    pub tokenizer: Option<TokenizerProfile>,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            margin_pct: 2.0,
            max_input_bytes: 2 * 1024 * 1024,
            format_hint: None,
            precise_tokens: false,
            auto_margin: false,
            dict_enabled: true,
            critical_policy: CriticalFieldPolicy::default_policy(),
            tokenizer: None,
        }
    }
}

/// Policy for protecting "critical" fields from lossy-looking compression
/// transforms (#3). The dictionary tier is lossless, but abbreviating
/// identity/secret/signature columns can confuse downstream consumers and
/// audit tooling, so those columns are kept verbatim. Matching is
/// case-insensitive substring over the column/key name (deliberately
/// over-protective: an unrecognized field is safer left alone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticalFieldPolicy {
    /// Lower-cased column/key substrings that must never be abbreviated.
    pub protected: Vec<String>,
    /// A candidate abbreviation is only suppressed by the policy when it would
    /// also have produced at least this many bytes of savings per occurrence;
    /// tiny abbreviations are never worth the confusion regardless. Default 3.
    pub min_benefit_bytes: usize,
}

impl CriticalFieldPolicy {
    /// The constitution-default protection list: identity / security /
    /// signature-bearing columns are kept verbatim.
    pub fn default_policy() -> Self {
        Self {
            protected: vec![
                "id".to_string(),
                "uuid".to_string(),
                "guid".to_string(),
                "token".to_string(),
                "secret".to_string(),
                "password".to_string(),
                "hash".to_string(),
                "signature".to_string(),
                "checksum".to_string(),
                "key".to_string(),
                "email".to_string(),
                "ssn".to_string(),
                "api".to_string(),
            ],
            min_benefit_bytes: 3,
        }
    }

    /// Returns true when the column/key `name` is protected by this policy AND
    /// the suppressed abbreviation would have saved at least
    /// `benefit_bytes` per occurrence.
    pub fn is_protected(&self, name: &str, benefit_bytes: usize) -> bool {
        if benefit_bytes < self.min_benefit_bytes {
            return false;
        }
        let lower = name.to_lowercase();
        self.protected.iter().any(|p| lower.contains(p.as_str()))
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
    /// Columns/keys the critical-field policy (#3) protected from dictionary
    /// abbreviation in the conversion that *would* have been surfaced. Empty
    /// when no conversion was computed or nothing was protected.
    pub protected_fields: Vec<String>,
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
    /// Columns/keys the critical-field policy (#3) protected from dictionary
    /// abbreviation, if any.
    pub protected_fields: Vec<String>,
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
