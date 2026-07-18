// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned pipe [--margin <pct>] [--max-bytes <n>]`
//!
//! stdin -> `maybe_tooned` -> stdout. Composable primitive; passthrough on
//! any doubt (FR-006/FR-007). Always exits 0 (`contracts/cli.md`) -- even a
//! stdin/stdout I/O hiccup falls back to a best-effort no-op rather than a
//! non-zero exit, since this subcommand's whole contract is "never surprise
//! the caller with a hard failure".

use std::path::PathBuf;

use clap::Args;
use tooned_core::Conversion;

use crate::cli::FormatHint;
use crate::cli::io::{BoundedRead, read_bounded};

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

    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    // Bounded read: `--max-bytes`/`max_input_bytes` documents itself as
    // "before a hard passthrough", so stdin is never buffered past that cap
    // -- once it's known to be exceeded, `maybe_tooned`'s own
    // `InputTooLarge` gate guarantees an unchanged passthrough regardless of
    // content, so the remainder is streamed straight to stdout instead of
    // being accumulated (finding: unbounded `read_to_end` previously ran
    // before the cap was ever consulted).
    // Can't read stdin at all -- nothing sane to convert; stay silent and
    // still exit 0 per the contract.
    let Ok(outcome) = read_bounded(&mut stdin, opts.max_input_bytes, &mut stdout) else {
        return Ok(());
    };

    let bytes = match outcome {
        BoundedRead::Fits(bytes) => bytes,
        // Already streamed verbatim to stdout by `read_bounded`.
        BoundedRead::Streamed { .. } => return Ok(()),
    };

    #[allow(clippy::manual_unwrap_or)]
    let input_len = match bytes.len().try_into() {
        Ok(v) => v,
        Err(_) => i64::MAX,
    };
    let output = match tooned_core::maybe_tooned(&bytes, &opts) {
        Ok(Conversion::Toon { text, .. }) => text.into_bytes(),
        Ok(Conversion::Passthrough { bytes, .. }) => bytes,
        Err(_) => bytes,
    };
    #[allow(clippy::manual_unwrap_or)]
    let output_len = match output.len().try_into() {
        Ok(v) => v,
        Err(_) => i64::MAX,
    };
    let converted = output_len < input_len;
    crate::metrics_recorder::record_convert_outcome(
        crate::metrics_recorder::CliSurface::Pipe,
        &crate::metrics_recorder::SourceLabel::None,
        None,
        converted,
        input_len,
        output_len,
    );

    // Write to the requested destination (stdout by default). A broken pipe
    // on the reader side is not this subcommand's problem to escalate as a
    // CLI error either.
    let write_result = crate::cli::io::write_output(args.out.as_deref(), &output);
    if let (Some(_), Err(err)) = (args.out.as_ref(), write_result) {
        return Err(anyhow::anyhow!("tooned pipe: failed to write output: {err}"));
    }

    Ok(())
}
