//! `tooned convert <file|-> [--to toon|json] [--out <file|->]`
//!
//! One-shot conversion; stdout by default. Never mutates the source file
//! (FR-005): reads are always read-only, and `--out` writes to a distinct
//! destination, never back onto `input`.

use std::path::PathBuf;

use clap::Args;
use tooned_core::{Conversion, ConversionOptions, decode_toon, maybe_tooned};

use crate::cli::io::{read_input, write_output};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Direction {
    Toon,
    Json,
}

#[derive(Debug, Args)]
pub struct ConvertArgs {
    /// Input file, or `-` for stdin.
    pub input: PathBuf,

    /// Force conversion direction instead of the adaptive default.
    #[arg(long, value_enum)]
    pub to: Option<Direction>,

    /// Output destination, or `-` for stdout (default).
    #[arg(long)]
    pub out: Option<PathBuf>,
}

// `Result` is kept (rather than `()`) to match every other subcommand's
// `run` signature uniformly dispatched from `main.rs`, and because
// `convert` surfaces real I/O/decode errors (exit 2/3) via
// `std::process::exit` below rather than through the `Err` path.
#[allow(clippy::unnecessary_wraps)]
pub fn run(args: &ConvertArgs) -> anyhow::Result<()> {
    let bytes = match read_input(&args.input) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("tooned: failed to read {}: {err}", args.input.display());
            std::process::exit(2);
        }
    };

    let output = match args.to {
        Some(Direction::Json) => decode_to_json_or_exit(&bytes),
        Some(Direction::Toon) => {
            // `--to toon` forces the JSON->TOON direction, bypassing the
            // adaptive default's 2% savings cushion (margin_pct: 0.0) while
            // still honoring the never-regression/round-trip-fidelity
            // invariants (constitution Principle I/II) -- forced conversion
            // still falls back to passthrough rather than ever emitting a
            // corrupted or larger-than-source encoding.
            let opts = ConversionOptions { margin_pct: 0.0, ..ConversionOptions::default() };
            adaptive_bytes(&bytes, &opts)
        }
        None => adaptive_bytes(&bytes, &ConversionOptions::default()),
    };

    if let Err(err) = write_output(args.out.as_deref(), &output) {
        eprintln!("tooned: failed to write output: {err}");
        std::process::exit(2);
    }

    Ok(())
}

/// `--to json` forces treating `bytes` as raw TOON text, regardless of what
/// content-sniffing would otherwise guess. Unlike the adaptive JSON->TOON
/// path, an invalid-TOON decode here is a genuine contract-level failure
/// (not payload-driven ambiguity in the adaptive sense), so it exits 3
/// rather than silently passing through (`contracts/cli.md`).
fn decode_to_json_or_exit(bytes: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        eprintln!("tooned: input is not valid UTF-8 TOON text");
        std::process::exit(3);
    };
    match decode_toon(text) {
        Ok(value) => match serde_json::to_vec(&value) {
            Ok(json) => json,
            Err(err) => {
                eprintln!("tooned: decoded TOON has no JSON representation: {err}");
                std::process::exit(3);
            }
        },
        Err(err) => {
            eprintln!("tooned: failed to decode TOON: {err}");
            std::process::exit(3);
        }
    }
}

/// Runs the shared adaptive decision and returns the bytes to emit: TOON
/// text on a genuine conversion, or the original bytes verbatim on any
/// passthrough outcome (constitution Principle I -- never an error for
/// payload-driven ambiguity).
fn adaptive_bytes(bytes: &[u8], opts: &ConversionOptions) -> Vec<u8> {
    match maybe_tooned(bytes, opts) {
        Ok(Conversion::Toon { text, .. }) => text.into_bytes(),
        Ok(Conversion::Passthrough { bytes, .. }) => bytes,
        // Infallible in practice (see maybe_tooned's doc comment); fail
        // safe to the original bytes rather than panicking or erroring.
        Err(_) => bytes.to_vec(),
    }
}
