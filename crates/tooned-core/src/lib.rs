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

// Re-export public types from tooned-types
pub use tooned_types::{
    Conversion, ConversionOptions, ConversionReport, DocType, InspectReport, PassthroughReason,
    ShapeClass, ToonedError,
};

// Re-export public functions from tooned-convert
pub use tooned_convert::{inspect, maybe_tooned};

// Re-export decode_toon from tooned-toon
pub use tooned_toon::decode_toon;

// Re-export SONIC_RS_THRESHOLD_BYTES from tooned-json
pub use tooned_json::SONIC_RS_THRESHOLD_BYTES;

// Re-export XML module from tooned-xml
pub mod xml {
    pub use tooned_xml::{XmlParseOptions, parse, sniff};
}
