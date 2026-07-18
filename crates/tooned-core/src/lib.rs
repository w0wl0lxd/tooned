// SPDX-License-Identifier: AGPL-3.0-only

//! # tooned-core
//!
//! Doctype detection and adaptive TOON-vs-compact-JSON conversion.
//!
//! Dependency-minimal by design: no SQLite, no directory walking. This crate
//! is meant to be embedded directly in a latency-sensitive agent hook
//! process. See `tooned-index` for the on-disk `.tooned/` project index
//! and `tooned` for the distributed binary (CLI, hooks, MCP server)
//! that wires this crate together with `tooned-index`.
//!
//! The public surface is defined by `contracts/tooned-core-api.md`:
//! [`maybe_tooned`], [`maybe_tooned_in`], [`inspect`], and [`decode_toon`].
//! The zero-allocation hot path is [`maybe_tooned_in`]`/`[`toon_from_value`]`:
//! after parsing, the function writes TOON into a caller-provided `&mut String`
//! and returns a [`Conversion`] that borrows from it. The parsing step itself
//! still allocates, but the encode/verify/margin phases do not when dict,
//! auto-margin, and entropy tiers are disabled. Every other integration
//! surface (CLI, Claude Code/Codex hooks, MCP server) funnels through these
//! functions rather than re-implementing detection or conversion logic
//! (constitution Principle V).

// Re-export public types from tooned-types
pub use tooned_types::{
    Conversion, ConversionOptions, ConversionReport, DocType, InspectReport, PassthroughReason,
    ShapeClass, ToonedError,
};

// Re-export public functions from tooned-convert
pub use tooned_convert::onto::decode as decode_onto;
pub use tooned_convert::tron::{
    StreamStats, decode as decode_tron, encode as encode_tron, maybe_tron, maybe_tron_stream,
};
pub use tooned_convert::{
    encode_onto, inspect, is_smaller_enough, maybe_onto, maybe_tooned, maybe_tooned_in,
    parse_to_value, toon_from_value,
};

// Re-export decode_toon from tooned-toon
pub use tooned_toon::decode_toon;

// Re-export SONIC_RS_THRESHOLD_BYTES and streaming parsers from tooned-json
pub use tooned_json::{SONIC_RS_THRESHOLD_BYTES, parse_json_stream, parse_ndjson_stream};

// Re-export XML module from tooned-xml
pub mod xml {
    pub use tooned_xml::{XmlParseOptions, parse, sniff};
}

use std::path::{Path, PathBuf};

/// Nearest ancestor of (or including) `start` that contains a `.tooned/`
/// directory or a `flake.nix` file, used to locate a project-scoped index or
/// metrics ledger. Flake roots are treated as project roots so `tooned index`
/// works out of the box in Nix flake repositories. Falls back to `start`
/// itself when no ancestor qualifies (so callers can still operate on a cwd
/// that has not been indexed yet).
pub fn project_root(start: &Path) -> PathBuf {
    let mut dir = start;
    loop {
        if dir.join(".tooned").is_dir() {
            return dir.to_path_buf();
        }
        if dir.join("flake.nix").is_file() {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return start.to_path_buf(),
        }
    }
}
