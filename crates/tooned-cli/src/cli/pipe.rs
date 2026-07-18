// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned pipe [--margin <pct>] [--max-bytes <n>]`
//!
//! stdin -> `maybe_tooned` -> stdout. Composable primitive; passthrough on
//! any doubt (FR-006/FR-007). Always exits 0 (`contracts/cli.md`) -- even a
//! stdin/stdout I/O hiccup falls back to a best-effort no-op rather than a
//! non-zero exit, since this subcommand's whole contract is "never surprise
//! the caller with a hard failure".

use std::io::Write as _;
use std::path::{Path, PathBuf};

use clap::Args;
use tooned_core::Conversion;

use crate::cli::FormatHint;
use crate::cli::io::{BoundedRead, atomic_rename, open_output_temp, output_writer, read_bounded};

#[derive(Debug, Args)]
pub struct PipeArgs {
    /// Minimum savings margin, as a percentage, required to convert (default 2%).
    #[arg(short = 'm', long)]
    pub margin: Option<f64>,

    /// Maximum input size in bytes before hard passthrough (default 2 MiB).
    #[arg(short = 'b', long = "max-bytes")]
    pub max_bytes: Option<u64>,

    /// Force the parser's doc type instead of relying on content-sniffing.
    #[arg(short = 'f', long = "format-hint", value_enum)]
    pub format_hint: Option<FormatHint>,

    /// Path to a tooned config file.
    #[arg(short = 'c', long)]
    pub config: Option<PathBuf>,

    /// Write output to this file instead of stdout.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub out: Option<PathBuf>,
}

/// RAII guard that removes a temporary file unless explicitly disarmed.
struct TempGuard<'a> {
    path: &'a Path,
    armed: bool,
}

impl<'a> TempGuard<'a> {
    fn new(path: &'a Path) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(self.path);
        }
    }
}

#[allow(clippy::unnecessary_wraps)]
pub fn run(args: &PipeArgs) -> anyhow::Result<()> {
    let config = crate::config::Config::load(args.config.as_deref())?;
    let opts = config.conversion_options(
        args.margin,
        args.max_bytes,
        args.format_hint,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );

    let is_stdout = args.out.as_deref().is_none_or(|p| p == Path::new("-"));
    let mut stdin = std::io::stdin();

    if is_stdout {
        let mut out_writer = output_writer(args.out.as_deref())?;

        // Bounded read: `--max-bytes`/`max_input_bytes` documents itself as
        // "before a hard passthrough", so stdin is never buffered past that cap
        // -- once it's known to be exceeded, `maybe_tooned`'s own
        // `InputTooLarge` gate guarantees an unchanged passthrough regardless of
        // content, so the remainder is streamed straight to stdout instead of
        // being accumulated.
        // Nothing sane to convert; stay silent and still exit 0 per the
        // stdout contract.
        let Ok(outcome) = read_bounded(&mut stdin, opts.max_input_bytes, &mut out_writer) else {
            return Ok(());
        };

        let bytes = match outcome {
            BoundedRead::Fits(bytes) => bytes,
            // Already streamed verbatim to stdout by `read_bounded`; flush any
            // buffered output and return.
            BoundedRead::Streamed { .. } => {
                let _ = out_writer.flush();
                return Ok(());
            }
        };

        let output = maybe_tooned_output(&bytes, &opts);
        // A broken pipe on the reader side is not this subcommand's problem to
        // escalate as a CLI error.
        let _ = out_writer.write_all(&output);
        let _ = out_writer.flush();
        return Ok(());
    }

    // File destination: do NOT open/truncate the final file before stdin has
    // been read. Instead, stream into a same-directory temp file and promote it
    // with an atomic rename once the input is fully consumed. This keeps the
    // destination file intact (or absent) while reading, avoiding the case
    // where `tooned pipe --out data.json < data.json` truncates the input
    // before it is read.
    let Some(path) = args.out.as_deref() else {
        return Ok(());
    };
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (tmp_path, mut writer) = open_output_temp(&target)?;
    let mut guard = TempGuard::new(&tmp_path);

    let outcome = match read_bounded(&mut stdin, opts.max_input_bytes, &mut writer) {
        Ok(outcome) => outcome,
        Err(err) => {
            return Err(anyhow::anyhow!("tooned pipe: failed to read/process input: {err}"));
        }
    };

    match outcome {
        BoundedRead::Streamed { .. } => {
            writer
                .flush()
                .map_err(|err| anyhow::anyhow!("tooned pipe: failed to flush output: {err}"))?;
            atomic_rename(&tmp_path, &target)
                .map_err(|err| anyhow::anyhow!("tooned pipe: failed to write output: {err}"))?;
            guard.disarm();
            return Ok(());
        }
        BoundedRead::Fits(bytes) => {
            let output = maybe_tooned_output(&bytes, &opts);
            writer
                .write_all(&output)
                .map_err(|err| anyhow::anyhow!("tooned pipe: failed to write output: {err}"))?;
            writer
                .flush()
                .map_err(|err| anyhow::anyhow!("tooned pipe: failed to flush output: {err}"))?;
            atomic_rename(&tmp_path, &target)
                .map_err(|err| anyhow::anyhow!("tooned pipe: failed to write output: {err}"))?;
            guard.disarm();
        }
    }

    Ok(())
}

fn maybe_tooned_output(bytes: &[u8], opts: &tooned_core::ConversionOptions) -> Vec<u8> {
    #[allow(clippy::manual_unwrap_or)]
    let to_i64_or_max = |n: usize| match i64::try_from(n) {
        Ok(v) => v,
        Err(_) => i64::MAX,
    };

    let input_len = to_i64_or_max(bytes.len());
    let output = match tooned_core::maybe_tooned(bytes, opts) {
        Ok(Conversion::Toon { text, .. }) => text.into_bytes(),
        Ok(Conversion::Passthrough { bytes, .. }) => bytes,
        Err(_) => bytes.to_vec(),
    };
    let output_len = to_i64_or_max(output.len());
    let converted = output_len < input_len;
    crate::metrics_recorder::record_convert_outcome(
        crate::metrics_recorder::CliSurface::Pipe,
        &crate::metrics_recorder::SourceLabel::None,
        None,
        converted,
        input_len,
        output_len,
    );
    output
}
