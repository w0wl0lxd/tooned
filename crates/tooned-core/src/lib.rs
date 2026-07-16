// SPDX-License-Identifier: AGPL-3.0-only

//! # tooned-core
//!
//! Doctype detection and adaptive TOON-vs-compact-JSON conversion.
//!
//! Dependency-minimal by design: no SQLite, no directory walking. This crate
//! is meant to be embedded directly in a latency-sensitive agent hook
//! process. See `tooned-index` for the on-disk `.tooned/` project index
//! and `tooned-cli` for the distributed binary (CLI, hooks, MCP server)
//! that wires this crate together with `tooned-index`.
//!
//! The public surface here is exactly `contracts/tooned-core-api.md`:
//! [`maybe_tooned`], [`inspect`], and [`decode_toon`]. Every other
//! integration surface (CLI, Claude Code/Codex hooks, MCP server) funnels
//! through these three functions rather than re-implementing detection or
//! conversion logic (constitution Principle V).

mod convert;
mod detect;
mod error;
mod parse;
mod shape;
pub mod xml;

pub use convert::{Conversion, ConversionReport, InspectReport, inspect, maybe_tooned};
pub use error::{ToonedError, decode_toon};
pub use parse::SONIC_RS_THRESHOLD_BYTES;
pub use shape::ShapeClass;

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

/// Tunables for [`maybe_tooned`]/[`inspect`]. See [`ConversionOptions::default`]
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

/// Why `maybe_tooned` did not surface a `Toon` conversion (or, when it did,
/// the internal decision path an equivalent `PassthroughReason` would have
/// taken -- see `InspectReport::reason`).
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
