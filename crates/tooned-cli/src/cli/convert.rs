// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned convert <file|-> [--to toon|json] [--out <file|->]`
//!
//! One-shot conversion; stdout by default. Never mutates the source file
//! (FR-005): reads are always read-only, and `--out` writes to a distinct
//! destination, never back onto `input`.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use clap::Args;
use tooned_core::{Conversion, ConversionOptions, decode_toon, maybe_tooned};

use crate::cli::FormatHint;
use crate::cli::io::{
    BoundedRead, atomic_rename, open_input, open_output_temp, read_bounded, read_input,
    write_atomic, write_output,
};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Direction {
    Toon,
    Json,
    Onto,
    Tron,
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

    /// Force the parser's doc type instead of relying on content-sniffing
    /// (only applies to the adaptive default and `--to toon`; `--to json`
    /// always decodes TOON regardless).
    #[arg(long = "format-hint", value_enum)]
    pub format_hint: Option<FormatHint>,

    /// Minimum savings margin, as a percentage, required to convert (default 2%).
    #[arg(long)]
    pub margin: Option<f64>,

    /// Maximum input size in bytes before hard passthrough (default 2 MiB).
    #[arg(long = "max-bytes")]
    pub max_bytes: Option<u64>,

    /// Path to a tooned config file.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

// `Result` is kept (rather than `()`) to match every other subcommand's
// `run` signature uniformly dispatched from `main.rs`, and because
// `convert` surfaces real I/O/decode errors (exit 2/3) via
// `std::process::exit` below rather than through the `Err` path.
#[allow(clippy::unnecessary_wraps)]
pub fn run(args: &ConvertArgs) -> anyhow::Result<()> {
    match args.to {
        // Decoding has no `max_input_bytes` gate of its own (unlike the
        // adaptive paths below) -- the whole file must be read regardless of
        // size to decode it correctly, so this direction keeps the simple
        // unbounded `read_input`/`write_output` path. When the output
        // destination is the same file as the input, use the same atomic
        // in-place write path as the adaptive conversion to avoid leaving a
        // partially-written source on a crash.
        Some(Direction::Json) => {
            let bytes = match read_input(&args.input) {
                Ok(bytes) => bytes,
                Err(err) => {
                    eprintln!("tooned convert: failed to read {}: {err}", args.input.display());
                    std::process::exit(2);
                }
            };
            let output = decode_to_json_or_exit(&bytes);
            let write_result = if output_is_same_as_input(&args.input, args.out.as_deref()) {
                write_in_place(&args.input, &output)
            } else {
                write_output(args.out.as_deref(), &output)
            };
            if let Err(err) = write_result {
                eprintln!("tooned convert: failed to write output: {err}");
                std::process::exit(2);
            }
        }
        // `--to onto` forces JSON-like input into the prototype ONTO
        // columnar encoding. It requires a uniform array of flat objects.
        // Like `--to toon`, the margin is forced to 0% but round-trip
        // fidelity is still enforced.
        Some(Direction::Onto) => {
            let bytes = match read_input(&args.input) {
                Ok(bytes) => bytes,
                Err(err) => {
                    eprintln!("tooned convert: failed to read {}: {err}", args.input.display());
                    std::process::exit(2);
                }
            };
            let config = crate::config::Config::load(args.config.as_deref())?;
            let mut opts =
                config.conversion_options(args.margin, args.max_bytes, args.format_hint, None);
            opts.margin_pct = 0.0;
            let output = match tooned_core::maybe_onto(&bytes, &opts) {
                Ok(Conversion::Toon { text, .. }) => text.into_bytes(),
                Ok(Conversion::Passthrough { bytes, .. }) => bytes,
                Err(_) => bytes.clone(),
            };
            let write_result = if output_is_same_as_input(&args.input, args.out.as_deref()) {
                write_in_place(&args.input, &output)
            } else {
                write_output(args.out.as_deref(), &output)
            };
            if let Err(err) = write_result {
                eprintln!("tooned convert: failed to write output: {err}");
                std::process::exit(2);
            }
        }
        // `--to tron` is a placeholder for the TRON record-stream encoding.
        Some(Direction::Tron) => {
            eprintln!("tooned convert: TRON encoding is not yet implemented");
            std::process::exit(2);
        }
        // `--to toon` forces the JSON->TOON direction, bypassing the
        // adaptive default's 2% savings cushion (margin_pct: 0.0) while
        // still honoring the never-regression/round-trip-fidelity
        // invariants (constitution Principle I/II) -- forced conversion
        // still falls back to passthrough rather than ever emitting a
        // corrupted or larger-than-source encoding.
        Some(Direction::Toon) => {
            let config = crate::config::Config::load(args.config.as_deref())?;
            let mut opts =
                config.conversion_options(args.margin, args.max_bytes, args.format_hint, None);
            // `--to toon` forces conversion with no savings margin.
            opts.margin_pct = 0.0;
            run_adaptive_bounded(args, &opts)?;
        }
        None => {
            let config = crate::config::Config::load(args.config.as_deref())?;
            let opts =
                config.conversion_options(args.margin, args.max_bytes, args.format_hint, None);
            run_adaptive_bounded(args, &opts)?;
        }
    }

    Ok(())
}

/// Returns `true` when the requested output destination is the same file as
/// the input, including via symlinks, hardlinks, or different relative paths
/// that resolve to the same inode. Stdin/stdout (`-`) is never considered the
/// same file.
fn output_is_same_as_input(input: &Path, out: Option<&Path>) -> bool {
    let Some(out) = out else { return false };
    if input == Path::new("-") || out == Path::new("-") {
        return false;
    }
    if input == out {
        return true;
    }
    if let Ok(true) = same_file::is_same_file(input, out) {
        return true;
    }
    match (std::fs::canonicalize(input), std::fs::canonicalize(out)) {
        (Ok(cin), Ok(cout)) => cin == cout,
        _ => false,
    }
}

/// Adaptive conversion when `--out` points at the same file as `input`.
/// Reads the source fully before opening the destination, so `File::create`
/// cannot truncate the input before it is consumed (FR-005). If the file is
/// already larger than `max_input_bytes`, the conversion would pass through
/// unchanged, so no write is performed.
#[allow(clippy::unnecessary_wraps)]
fn run_adaptive_in_place(input: &Path, opts: &ConversionOptions) -> anyhow::Result<()> {
    if let Ok(meta) = std::fs::metadata(input)
        && meta.len() > opts.max_input_bytes as u64
    {
        return Ok(());
    }

    let mut reader = match open_input(input) {
        Ok(reader) => reader,
        Err(err) => {
            eprintln!("tooned convert: failed to read {}: {err}", input.display());
            std::process::exit(2);
        }
    };

    // `read_bounded` writes to a sink; if the file is oversized, it streams
    // the remainder without materialising it in memory.
    let mut sink = std::io::sink();
    let outcome = match read_bounded(reader.as_mut(), opts.max_input_bytes, &mut sink) {
        Ok(outcome) => outcome,
        Err(err) => {
            eprintln!("tooned convert: failed to read {}: {err}", input.display());
            std::process::exit(2);
        }
    };

    let bytes = match outcome {
        BoundedRead::Fits(bytes) => bytes,
        BoundedRead::Streamed { .. } => return Ok(()),
    };

    let output = adaptive_bytes(&bytes, opts);
    if let Err(err) = write_in_place(input, &output) {
        eprintln!("tooned convert: failed to write output: {err}");
        std::process::exit(2);
    }

    Ok(())
}

/// Writes `output` back to `input` when they are the same file. Uses an
/// atomic temp-file-then-rename whenever the file has only one link, so a
/// crash cannot leave a partially-written source. Files with multiple
/// hardlinks cannot be atomically replaced without breaking the link, so
/// those fall back to an in-place `fs::write` (the source was already fully
/// read before this point, so truncation mid-process is not a concern).
fn write_in_place(input: &Path, output: &[u8]) -> std::io::Result<()> {
    let target = match std::fs::canonicalize(input) {
        Ok(canonical) => canonical,
        Err(_) => input.to_path_buf(),
    };
    if nlink(&target).is_some_and(|n| n > 1) {
        std::fs::write(&target, output)
    } else {
        write_atomic(input, output)
    }
}

/// Returns the number of hard links for `path`, or `None` if it cannot be
/// determined on the current platform.
fn nlink(path: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(path).ok()?;
        Some(meta.nlink())
    }
    #[cfg(windows)]
    {
        // `std::os::windows::fs::MetadataExt::number_of_links` is a nightly
        // feature (`windows_by_handle`). Open the file and query it through
        // `winapi_util` on stable Rust instead.
        let file = std::fs::File::open(path).ok()?;
        Some(winapi_util::file::information(&file).ok()?.number_of_links())
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Shared bounded-read path for both the default adaptive direction and
/// `--to toon`: both go through `maybe_tooned`, whose `InputTooLarge` gate
/// makes "larger than `opts.max_input_bytes`" and "guaranteed unchanged
/// passthrough" equivalent -- so the input is never fully buffered in
/// memory when it's oversized (finding: unbounded `read_to_end`/`fs::read`
/// previously ran before that size cap was ever consulted).
//
// For a file destination the output is staged through a same-directory temp
// file and promoted with a single `rename`, so a crash can never leave the
// destination partially written. stdout (`-` or `None`) writes directly.
// `Result` is kept for uniformity with `run` (see its own comment on the
// same trade-off); every failure path below exits the process directly.
#[allow(clippy::unnecessary_wraps)]
fn run_adaptive_bounded(args: &ConvertArgs, opts: &ConversionOptions) -> anyhow::Result<()> {
    if output_is_same_as_input(&args.input, args.out.as_deref()) {
        return run_adaptive_in_place(&args.input, opts);
    }

    let mut reader = match open_input(&args.input) {
        Ok(reader) => reader,
        Err(err) => {
            eprintln!("tooned convert: failed to read {}: {err}", args.input.display());
            std::process::exit(2);
        }
    };

    let out_path = args.out.as_deref();
    let (tmp_path, mut out): (Option<PathBuf>, Box<dyn std::io::Write>) = match out_path {
        None => (None, Box::new(std::io::stdout())),
        Some(p) if p == Path::new("-") => (None, Box::new(std::io::stdout())),
        Some(p) => {
            let (tmp, file) = open_output_temp(p)?;
            (Some(tmp), file)
        }
    };

    let outcome = match read_bounded(reader.as_mut(), opts.max_input_bytes, out.as_mut()) {
        Ok(outcome) => outcome,
        Err(err) => {
            eprintln!("tooned convert: failed to read {}: {err}", args.input.display());
            if let Some(tmp) = &tmp_path {
                let _ = std::fs::remove_file(tmp);
            }
            std::process::exit(2);
        }
    };

    match outcome {
        BoundedRead::Fits(bytes) => {
            let output = adaptive_bytes(&bytes, opts);
            if let Err(err) = out.write_all(&output) {
                eprintln!("tooned convert: failed to write output: {err}");
                if let Some(tmp) = &tmp_path {
                    let _ = std::fs::remove_file(tmp);
                }
                std::process::exit(2);
            }
        }
        // `read_bounded` already streamed the original bytes to `out`; for
        // stdout there is nothing more to do, for a file we just rename.
        BoundedRead::Streamed { .. } => {}
    }

    if let (Some(tmp), Some(target)) = (tmp_path, out_path)
        && let Err(err) = atomic_rename(&tmp, target)
    {
        eprintln!("tooned convert: failed to write output: {err}");
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
        eprintln!("tooned convert: input is not valid UTF-8 TOON text");
        std::process::exit(3);
    };
    match decode_toon(text) {
        Ok(value) => match serde_json::to_vec(&value) {
            Ok(json) => json,
            Err(err) => {
                eprintln!("tooned convert: decoded TOON has no JSON representation: {err}");
                std::process::exit(3);
            }
        },
        Err(err) => {
            eprintln!("tooned convert: failed to decode TOON: {err}");
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
