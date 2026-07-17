// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned convert <file|-> [--to toon|json] [--out <file|->]`
//!
//! One-shot conversion; stdout by default. Never mutates the source file
//! (FR-005): reads are always read-only, and `--out` writes to a distinct
//! destination, never back onto `input`.

use std::io::{BufRead, Write as _};
use std::path::{Path, PathBuf};

use clap::Args;
use tooned_core::{
    Conversion, ConversionOptions, StreamStats, decode_onto, decode_toon, decode_tron,
    is_smaller_enough, maybe_tooned, maybe_tron, maybe_tron_stream,
};

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
    let config = crate::config::Config::load(args.config.as_deref())?;
    let format_hint = args.format_hint.or_else(|| config.format_hint()).or_else(|| {
        args.input
            .extension()
            .and_then(|e| e.to_str())
            .and_then(crate::cli::format_hint_from_extension)
    });

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
            let mut opts =
                config.conversion_options(args.margin, args.max_bytes, format_hint, None);
            opts.margin_pct = 0.0;
            let onto_outcome = tooned_core::maybe_onto(&bytes, &opts);
            let converted = matches!(onto_outcome, Ok(Conversion::Toon { .. }));
            let output = match onto_outcome {
                Ok(Conversion::Toon { text, .. }) => text.into_bytes(),
                Ok(Conversion::Passthrough { bytes, .. }) => bytes,
                Err(_) => bytes.clone(),
            };
            #[allow(clippy::manual_unwrap_or)]
            crate::metrics_recorder::record_convert_outcome(
                crate::metrics_recorder::CliSurface::Onto,
                &crate::metrics_recorder::label_from_path(&args.input),
                format_hint.map(std::convert::Into::into),
                converted,
                match bytes.len().try_into() {
                    Ok(v) => v,
                    Err(_) => i64::MAX,
                },
                match output.len().try_into() {
                    Ok(v) => v,
                    Err(_) => i64::MAX,
                },
            );
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
        // `--to tron` forces JSON-like input into the prototype TRON
        // record-stream encoding. It requires a uniform object/array of flat
        // objects. Like `--to onto`, the margin is forced to 0% but round-trip
        // fidelity is still enforced.
        Some(Direction::Tron) => {
            let mut opts =
                config.conversion_options(args.margin, args.max_bytes, format_hint, None);
            opts.margin_pct = 0.0;

            // Check if we should use streaming for NDJSON input
            let input_size = get_input_size(&args.input);
            let is_ndjson = format_hint == Some(FormatHint::Ndjson);
            let use_streaming = is_ndjson || input_size > opts.max_input_bytes as u64;

            if use_streaming && is_ndjson {
                // Stream NDJSON to TRON
                let result = run_tron_streaming(args, &opts);
                if let Err(err) = result {
                    eprintln!("tooned convert: failed to stream TRON: {err}");
                    std::process::exit(2);
                }
            } else {
                // Use the existing bounded path for non-NDJSON or small inputs
                let bytes = match read_input(&args.input) {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        eprintln!("tooned convert: failed to read {}: {err}", args.input.display());
                        std::process::exit(2);
                    }
                };
                let tron_outcome = maybe_tron(&bytes, &opts);
                let converted = matches!(tron_outcome, Ok(Conversion::Toon { .. }));
                let output = match tron_outcome {
                    Ok(Conversion::Toon { text, .. }) => text.into_bytes(),
                    Ok(Conversion::Passthrough { bytes, .. }) => bytes,
                    Err(_) => bytes.clone(),
                };
                #[allow(clippy::manual_unwrap_or)]
                crate::metrics_recorder::record_convert_outcome(
                    crate::metrics_recorder::CliSurface::Tron,
                    &crate::metrics_recorder::label_from_path(&args.input),
                    format_hint.map(std::convert::Into::into),
                    converted,
                    match bytes.len().try_into() {
                        Ok(v) => v,
                        Err(_) => i64::MAX,
                    },
                    match output.len().try_into() {
                        Ok(v) => v,
                        Err(_) => i64::MAX,
                    },
                );
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
        }
        // `--to toon` forces the JSON->TOON direction, bypassing the
        // adaptive default's 2% savings cushion (margin_pct: 0.0) while
        // still honoring the never-regression/round-trip-fidelity
        // invariants (constitution Principle I/II) -- forced conversion
        // still falls back to passthrough rather than ever emitting a
        // corrupted or larger-than-source encoding.
        Some(Direction::Toon) => {
            let mut opts =
                config.conversion_options(args.margin, args.max_bytes, format_hint, None);
            // `--to toon` forces conversion with no savings margin.
            opts.margin_pct = 0.0;
            run_adaptive_bounded(args, &opts)?;
        }
        None => {
            let opts = config.conversion_options(args.margin, args.max_bytes, format_hint, None);

            // Stream only when the input is genuinely too large to buffer:
            // `maybe_tooned` (via `run_adaptive_bounded`) is the single,
            // correct adaptive decision for any input that fits in memory,
            // picking TOON when it beats compact JSON by the margin and
            // otherwise passing through. Routing bounded NDJSON through it
            // (rather than the streaming TRON-only path) avoids a savings
            // regression where TOON would have won. Streaming TRON remains
            // the fallback for inputs that exceed `max_input_bytes`.
            let input_size = get_input_size(&args.input);
            let is_ndjson = format_hint == Some(FormatHint::Ndjson);
            let use_streaming = is_ndjson && input_size > opts.max_input_bytes as u64;

            if use_streaming && is_ndjson {
                // Stream NDJSON to TRON with adaptive size check
                let result = run_adaptive_streaming(args, &opts);
                if let Err(err) = result {
                    eprintln!("tooned convert: failed to stream adaptive conversion: {err}");
                    std::process::exit(2);
                }
            } else {
                run_adaptive_bounded(args, &opts)?;
            }
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

/// `--to json` forces decoding the input as raw TOON, ONTO (when the text
/// starts with the `!schema ` header), or TRON (when the text starts with a
/// `class ` header). Unlike the adaptive JSON->TOON path, an invalid decode
/// here is a genuine contract-level failure (not payload-driven ambiguity in
/// the adaptive sense), so it exits 3 rather than silently passing through
/// (`contracts/cli.md`).
///
/// TOON is tried first because a plain TOON document can itself begin with a
/// `class ` key (e.g. `class Foo { ... }`), which would otherwise be mistaken
/// for a TRON record header. ONTO/TRON are only attempted once their explicit
/// schema prefix is present.
fn decode_to_json_or_exit(bytes: &[u8]) -> Vec<u8> {
    const ONTO_SCHEMA_PREFIX: &str = "!schema ";
    const TRON_CLASS_PREFIX: &str = "class ";

    let Ok(text) = std::str::from_utf8(bytes) else {
        eprintln!("tooned convert: input is not valid UTF-8 text");
        std::process::exit(3);
    };

    // Try TOON first: it is the most general encoding and never collides with
    // a `class `/`!schema ` prefix (those are ONTO/TRON markers, not valid
    // TOON headers).
    if !text.starts_with(ONTO_SCHEMA_PREFIX)
        && !text.trim_start().starts_with(TRON_CLASS_PREFIX)
        && let Ok(value) = decode_toon(text)
    {
        return finalize_json(&value, bytes);
    }

    let value = if text.starts_with(ONTO_SCHEMA_PREFIX) {
        match decode_onto(text) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("tooned convert: failed to decode ONTO: {err}");
                std::process::exit(3);
            }
        }
    } else if text.trim_start().starts_with(TRON_CLASS_PREFIX) {
        match decode_tron(text) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("tooned convert: failed to decode TRON: {err}");
                std::process::exit(3);
            }
        }
    } else {
        match decode_toon(text) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("tooned convert: failed to decode TOON: {err}");
                std::process::exit(3);
            }
        }
    };

    finalize_json(&value, bytes)
}

/// Emit `value` as compact JSON, recording the decode outcome for metrics.
fn finalize_json(value: &serde_json::Value, bytes: &[u8]) -> Vec<u8> {
    match sonic_rs::to_vec(value) {
        Ok(json) => {
            #[allow(clippy::manual_unwrap_or)]
            crate::metrics_recorder::record_convert_outcome(
                crate::metrics_recorder::CliSurface::Decode,
                &crate::metrics_recorder::SourceLabel::None,
                None,
                true,
                match bytes.len().try_into() {
                    Ok(v) => v,
                    Err(_) => i64::MAX,
                },
                match json.len().try_into() {
                    Ok(v) => v,
                    Err(_) => i64::MAX,
                },
            );
            json
        }
        Err(err) => {
            eprintln!("tooned convert: decoded text has no JSON representation: {err}");
            std::process::exit(3);
        }
    }
}

/// Runs the shared adaptive decision and returns the bytes to emit: TOON
/// text on a genuine conversion, or the original bytes verbatim on any
/// passthrough outcome (constitution Principle I -- never an error for
/// payload-driven ambiguity).
fn adaptive_bytes(bytes: &[u8], opts: &ConversionOptions) -> Vec<u8> {
    #[allow(clippy::manual_unwrap_or)]
    let input_len = match bytes.len().try_into() {
        Ok(v) => v,
        Err(_) => i64::MAX,
    };
    let output = match maybe_tooned(bytes, opts) {
        Ok(Conversion::Toon { text, .. }) => text.into_bytes(),
        Ok(Conversion::Passthrough { bytes, .. }) => bytes,
        // Infallible in practice (see maybe_tooned's doc comment); fail
        // safe to the original bytes rather than panicking or erroring.
        Err(_) => bytes.to_vec(),
    };
    #[allow(clippy::manual_unwrap_or)]
    let output_len = match output.len().try_into() {
        Ok(v) => v,
        Err(_) => i64::MAX,
    };
    let converted = output_len != input_len;
    crate::metrics_recorder::record_convert_outcome(
        crate::metrics_recorder::CliSurface::Convert,
        &crate::metrics_recorder::SourceLabel::None,
        opts.format_hint,
        converted,
        input_len,
        output_len,
    );
    output
}

/// Returns the size of the input in bytes, or 0 for stdin (unknown size).
fn get_input_size(input: &Path) -> u64 {
    if input == Path::new("-") {
        return 0;
    }
    std::fs::metadata(input).map_or(0, |m| m.len())
}

/// Guard for a temporary file. Deletes the file on drop unless the path is
/// explicitly taken via `into_path`.
struct TempFile(Option<PathBuf>);

impl TempFile {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn path(&self) -> &Path {
        match &self.0 {
            Some(p) => p,
            None => Path::new(""),
        }
    }

    fn into_path(mut self) -> PathBuf {
        match self.0.take() {
            Some(p) => p,
            None => PathBuf::new(),
        }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Returns a unique temporary path in `dir` with the given `prefix`.
fn unique_temp_path(dir: &Path, prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    dir.join(format!(".{prefix}.{}.{}.tmp", std::process::id(), nanos))
}

/// Spools stdin to a uniquely-named temp file and returns the temp file guard,
/// an open `File` for reading, and the number of bytes copied.
fn spool_stdin_to_temp() -> anyhow::Result<(TempFile, std::fs::File, u64)> {
    let path = unique_temp_path(&std::env::temp_dir(), "tooned-stdin");
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
    let mut stdin = std::io::stdin().lock();
    let size = std::io::copy(&mut stdin, &mut file)?;
    drop(file);
    let file = std::fs::File::open(&path)?;
    Ok((TempFile::new(path), file, size))
}

/// Opens a temp output writer. For a file destination the temp is created in
/// the same directory and can be atomically renamed; for stdout it is created
/// in the system temp directory and copied on success.
fn open_streaming_output(
    out: Option<&Path>,
) -> anyhow::Result<(TempFile, Box<dyn std::io::Write>)> {
    match out {
        Some(p) if p != Path::new("-") => {
            let (tmp_path, file) = open_output_temp(p)?;
            Ok((TempFile::new(tmp_path), Box::new(std::io::BufWriter::new(file))))
        }
        _ => {
            let path = unique_temp_path(&std::env::temp_dir(), "tooned-out");
            let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
            Ok((TempFile::new(path), Box::new(std::io::BufWriter::new(file))))
        }
    }
}

/// Copies `input` to the requested output destination without buffering the
/// whole file in memory. Skips the copy when the output is the same file as
/// the input.
fn copy_input_to_output(input: &Path, out: Option<&Path>) -> anyhow::Result<()> {
    if let Some(out) = out {
        if out == Path::new("-") {
            let mut src = std::fs::File::open(input)?;
            std::io::copy(&mut src, &mut std::io::stdout())?;
            return Ok(());
        }
        if output_is_same_as_input(input, Some(out)) {
            return Ok(());
        }
        std::fs::copy(input, out)?;
    } else {
        let mut src = std::fs::File::open(input)?;
        std::io::copy(&mut src, &mut std::io::stdout())?;
    }
    Ok(())
}

/// Runs streaming TRON conversion for `--to tron` on NDJSON input.
/// Streams to a temp file, then promotes it atomically for file output
/// or copies to stdout for stdout output. Falls back to passthrough on
/// parse/IO errors.
fn run_tron_streaming(args: &ConvertArgs, _opts: &ConversionOptions) -> anyhow::Result<()> {
    let out_path = args.out.as_deref();

    let (stdin_tmp, mut reader): (Option<TempFile>, Box<dyn BufRead>) =
        if args.input == Path::new("-") {
            let (tmp, file, _size) = spool_stdin_to_temp()?;
            (Some(tmp), Box::new(std::io::BufReader::new(file)))
        } else {
            let file = std::fs::File::open(&args.input)?;
            (None, Box::new(std::io::BufReader::new(file)))
        };
    let input_path: PathBuf =
        if let Some(tmp) = &stdin_tmp { tmp.path().to_path_buf() } else { args.input.clone() };

    let (output_tmp, mut out) = open_streaming_output(out_path)?;

    let stream_result = maybe_tron_stream(&mut *reader, &mut out);
    let flush_result = out.flush();

    if stream_result.is_err() || flush_result.is_err() {
        drop(out);
        copy_input_to_output(&input_path, out_path)?;
        return Ok(());
    }

    drop(out);

    match out_path {
        Some(p) if p != Path::new("-") => {
            let tmp_path = output_tmp.into_path();
            atomic_rename(&tmp_path, p)?;
        }
        _ => {
            let mut src = std::fs::File::open(output_tmp.path())?;
            std::io::copy(&mut src, &mut std::io::stdout())?;
        }
    }

    Ok(())
}

/// Runs adaptive streaming conversion for the default path.
/// Streams NDJSON to TRON in a temp file, then compares output size
/// vs input size using the margin check. If not smaller enough,
/// discards the temp and passthrough the original input.
fn run_adaptive_streaming(args: &ConvertArgs, opts: &ConversionOptions) -> anyhow::Result<()> {
    let out_path = args.out.as_deref();

    let (stdin_tmp, mut reader, input_size): (Option<TempFile>, Box<dyn BufRead>, u64) =
        if args.input == Path::new("-") {
            let (tmp, file, size) = spool_stdin_to_temp()?;
            (Some(tmp), Box::new(std::io::BufReader::new(file)), size)
        } else {
            let size = get_input_size(&args.input);
            let file = std::fs::File::open(&args.input)?;
            (None, Box::new(std::io::BufReader::new(file)), size)
        };
    let input_path: PathBuf =
        if let Some(tmp) = &stdin_tmp { tmp.path().to_path_buf() } else { args.input.clone() };

    let (output_tmp, mut out) = open_streaming_output(out_path)?;

    let stream_result = maybe_tron_stream(&mut *reader, &mut out);
    let flush_result = out.flush();

    if stream_result.is_err() || flush_result.is_err() {
        drop(out);
        copy_input_to_output(&input_path, out_path)?;
        return Ok(());
    }

    let StreamStats { output_bytes, .. } = stream_result?;
    let output_size = output_bytes as usize;

    drop(out);

    if is_smaller_enough(input_size as usize, output_size, opts.margin_pct) {
        match out_path {
            Some(p) if p != Path::new("-") => {
                let tmp_path = output_tmp.into_path();
                atomic_rename(&tmp_path, p)?;
            }
            _ => {
                let mut src = std::fs::File::open(output_tmp.path())?;
                std::io::copy(&mut src, &mut std::io::stdout())?;
            }
        }
    } else {
        copy_input_to_output(&input_path, out_path)?;
    }

    Ok(())
}
