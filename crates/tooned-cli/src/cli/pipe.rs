//! `tooned pipe [--margin <pct>] [--max-bytes <n>]`
//!
//! stdin -> `maybe_tooned` -> stdout. Composable primitive; passthrough on
//! any doubt (FR-006/FR-007). Always exits 0 (`contracts/cli.md`) -- even a
//! stdin/stdout I/O hiccup falls back to a best-effort no-op rather than a
//! non-zero exit, since this subcommand's whole contract is "never surprise
//! the caller with a hard failure".

use std::io::Write as _;

use clap::Args;
use tooned_core::{Conversion, ConversionOptions};

use crate::cli::FormatHint;
use crate::cli::io::{BoundedRead, read_bounded};

#[derive(Debug, Args)]
pub struct PipeArgs {
    /// Minimum savings margin, as a percentage, required to convert (default 2%).
    #[arg(long)]
    pub margin: Option<f64>,

    /// Maximum input size in bytes before hard passthrough (default 2 MiB).
    #[arg(long = "max-bytes")]
    pub max_bytes: Option<u64>,

    /// Force the parser's doc type instead of relying on content-sniffing.
    #[arg(long = "format-hint", value_enum)]
    pub format_hint: Option<FormatHint>,
}

#[allow(clippy::unnecessary_wraps)]
pub fn run(args: &PipeArgs) -> anyhow::Result<()> {
    let mut opts = ConversionOptions::default();
    if let Some(margin) = args.margin {
        opts.margin_pct = margin;
    }
    if let Some(format_hint) = args.format_hint {
        opts.format_hint = Some(format_hint.into());
    }
    if let Some(max_bytes) = args.max_bytes {
        // Defensive clamp rather than a fallible conversion: an
        // absurdly large --max-bytes on a 32-bit target simply saturates
        // to usize::MAX (still a "no effective limit" outcome), never a
        // panic or CLI error.
        opts.max_input_bytes = match usize::try_from(max_bytes) {
            Ok(clamped) => clamped,
            Err(_) => usize::MAX,
        };
    }

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

    let output = match tooned_core::maybe_tooned(&bytes, &opts) {
        Ok(Conversion::Toon { text, .. }) => text.into_bytes(),
        Ok(Conversion::Passthrough { bytes, .. }) => bytes,
        Err(_) => bytes,
    };

    // Best-effort write: a broken pipe on the reader side is not this
    // subcommand's problem to escalate as a CLI error either.
    let _ = stdout.write_all(&output);

    Ok(())
}
