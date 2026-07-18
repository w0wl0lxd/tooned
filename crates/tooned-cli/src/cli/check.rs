// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned check <file|-> [--precise]`
//!
//! Dry-run: prints doc type, shape class, byte-size comparison, convertible
//! y/n. Never writes converted output (no `maybe_tooned`/TOON-text call at
//! all -- only `tooned_core::inspect`, which by contract never computes or
//! returns TOON text).

use std::path::PathBuf;

use clap::Args;
use tooned_core::{InspectReport, PassthroughReason, ShapeClass, inspect};

use crate::cli::FormatHint;
use crate::cli::io::{BoundedRead, open_input, read_bounded};

#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Input file, or `-` for stdin.
    pub input: PathBuf,

    /// Additionally report BPE-token-based savings (opt-in, FR-023).
    #[arg(short = 'p', long)]
    pub precise: bool,

    /// Force the parser's doc type instead of relying on content-sniffing.
    #[arg(short = 'f', long = "format-hint", value_enum)]
    pub format_hint: Option<FormatHint>,

    /// Minimum savings margin, as a percentage, required to convert (default 2%).
    #[arg(short = 'm', long)]
    pub margin: Option<f64>,

    /// Maximum input size in bytes before hard passthrough (default 2 MiB).
    #[arg(short = 'b', long = "max-bytes")]
    pub max_bytes: Option<u64>,

    /// Path to a tooned config file.
    #[arg(short = 'c', long)]
    pub config: Option<PathBuf>,

    /// Emit the report as machine-readable JSON.
    #[arg(short = 'j', long)]
    pub json: bool,
}

// `Result` is kept (rather than `()`) to match every other subcommand's
// `run` signature uniformly dispatched from `main.rs`. Unlike `convert`,
// `check` is documented (`contracts/cli.md`) to exit 0 unconditionally -- "a
// 'not convertible' result is not a CLI error" -- with no I/O-error
// exception, so a read failure here must NOT hard-exit non-zero; it's
// reported on stdout/stderr and `run` still returns `Ok(())`.
#[allow(clippy::unnecessary_wraps)]
pub fn run(args: &CheckArgs) -> anyhow::Result<()> {
    let config = crate::config::Config::load(args.config.as_deref())?;
    let format_hint = args.format_hint.or_else(|| config.format_hint()).or_else(|| {
        args.input
            .extension()
            .and_then(|e| e.to_str())
            .and_then(crate::cli::format_hint_from_extension)
    });
    let precise = Some(args.precise || matches!(config.precise_tokens, Some(true)));
    let opts = config.conversion_options(
        args.margin,
        args.max_bytes,
        format_hint,
        precise,
        None,
        None,
        None,
        None,
    );

    let mut reader = match open_input(&args.input) {
        Ok(reader) => reader,
        Err(err) => {
            eprintln!("tooned check: failed to read {}: {err}", args.input.display());
            println!("convertible: no");
            return Ok(());
        }
    };

    // Bounded read: `check` never writes converted bytes anywhere, so an
    // oversized input's excess is discarded (`io::sink()`) rather
    // than ever being buffered in full -- `inspect`'s own `max_input_bytes`
    // gate makes "larger than cap" and "guaranteed InputTooLarge" equivalent
    // regardless of content (finding: unbounded `read_input` previously ran
    // before that size cap was ever consulted).
    let mut sink = std::io::sink();
    let outcome = match read_bounded(reader.as_mut(), opts.max_input_bytes, &mut sink) {
        Ok(outcome) => outcome,
        Err(err) => {
            eprintln!("tooned check: failed to read {}: {err}", args.input.display());
            println!("convertible: no");
            return Ok(());
        }
    };

    let report = match outcome {
        BoundedRead::Fits(bytes) => inspect(&bytes, &opts),
        BoundedRead::Streamed { total_bytes } => InspectReport {
            doc_type: None,
            shape: ShapeClass::Scalar,
            input_bytes: total_bytes as usize,
            json_bytes: None,
            toon_bytes: None,
            savings_pct: None,
            precise_savings_pct: None,
            would_convert: false,
            reason: Some(PassthroughReason::InputTooLarge),
            protected_fields: Vec::new(),
        },
    };

    if args.json {
        println!("{}", sonic_rs::to_string(&report)?);
        return Ok(());
    }

    let doc_type = match report.doc_type {
        Some(dt) => format!("{dt:?}"),
        None => "unknown".to_string(),
    };
    println!("doc type: {doc_type}");
    println!("shape: {:?}", report.shape);
    println!("input bytes: {}", report.input_bytes);
    if let Some(json_bytes) = report.json_bytes {
        println!("json bytes: {json_bytes}");
    } else {
        println!("json bytes: n/a");
    }
    if let Some(toon_bytes) = report.toon_bytes {
        println!("toon bytes: {toon_bytes}");
    } else {
        println!("toon bytes: n/a");
    }
    if let Some(savings_pct) = report.savings_pct {
        println!("savings: {savings_pct:.1}%");
    } else {
        println!("savings: n/a");
    }
    println!("convertible: {}", if report.would_convert { "yes" } else { "no" });
    if let Some(reason) = &report.reason {
        println!("reason: {reason:?}");
    }
    if args.precise {
        match report.precise_savings_pct {
            Some(precise_savings_pct) => {
                println!("precise (BPE-token) savings: {precise_savings_pct:.1}%");
            }
            None => println!("precise (BPE-token) savings: n/a"),
        }
    }

    #[allow(clippy::manual_unwrap_or)]
    let (input_bytes, output_bytes) = match (report.json_bytes, report.toon_bytes) {
        (Some(j), Some(t)) => (
            match j.try_into() {
                Ok(v) => v,
                Err(_) => i64::MAX,
            },
            match t.try_into() {
                Ok(v) => v,
                Err(_) => i64::MAX,
            },
        ),
        _ => (
            match report.input_bytes.try_into() {
                Ok(v) => v,
                Err(_) => i64::MAX,
            },
            match report.input_bytes.try_into() {
                Ok(v) => v,
                Err(_) => i64::MAX,
            },
        ),
    };
    crate::metrics_recorder::record_convert_outcome(
        crate::metrics_recorder::CliSurface::Check,
        &crate::metrics_recorder::label_from_path(&args.input),
        None,
        report.would_convert,
        input_bytes,
        output_bytes,
    );

    Ok(())
}
