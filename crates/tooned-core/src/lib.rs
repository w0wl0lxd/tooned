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
//! This is a scaffold: the conversion pipeline is implemented following the
//! spec-kit pipeline (`specs/`), not directly in this initial commit.

#[derive(Debug, thiserror::Error)]
pub enum ToonedError {
    #[error("input exceeds max_input_bytes limit")]
    InputTooLarge,
}
