// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned token-savings <file|->`
//!
//! Reports the byte and BPE-token savings that `tooned` would achieve on the
//! input, without emitting the TOON text itself. This is the dedicated
//! token-efficiency command backing the `--precise` analysis surfaced by
//! `tooned check`.

use std::path::PathBuf;

use clap::Args;
use tooned_core::{InspectReport, PassthroughReason, ShapeClass, inspect};

use crate::cli::FormatHint;
use crate::cli::io::{BoundedRead, open_input, read_bounded};

#[derive(Debug, Args)]
pub struct TokenSavingsArgs {
    /// Input file, or `-` for stdin.
    pub input: PathBuf,

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

#[allow(clippy::unnecessary_wraps)]
pub fn run(args: &TokenSavingsArgs) -> anyhow::Result<()> {
    let config = crate::config::Config::load(args.config.as_deref())?;
    let format_hint = args.format_hint.or_else(|| config.format_hint()).or_else(|| {
        args.input
            .extension()
            .and_then(|e| e.to_str())
            .and_then(crate::cli::format_hint_from_extension)
    });
    let opts = config.conversion_options(
        args.margin,
        args.max_bytes,
        format_hint,
        Some(true),
        None,
        None,
        None,
        None,
        None,
        None,
    );

    let mut reader = match open_input(&args.input) {
        Ok(reader) => reader,
        Err(err) => {
            eprintln!("tooned token-savings: failed to read {}: {err}", args.input.display());
            println!("would_convert: false");
            return Ok(());
        }
    };

    let mut sink = std::io::sink();
    let outcome = match read_bounded(reader.as_mut(), opts.max_input_bytes, &mut sink) {
        Ok(outcome) => outcome,
        Err(err) => {
            eprintln!("tooned token-savings: failed to read {}: {err}", args.input.display());
            println!("would_convert: false");
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
        println!("byte savings: {savings_pct:.1}%");
    } else {
        println!("byte savings: n/a");
    }
    match report.precise_savings_pct {
        Some(precise) => println!("bpe-token savings: {precise:.1}%"),
        None => println!("bpe-token savings: n/a"),
    }
    println!("would convert: {}", if report.would_convert { "yes" } else { "no" });
    if let Some(reason) = &report.reason {
        println!("reason: {reason:?}");
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
        crate::metrics_recorder::CliSurface::TokenSavings,
        &crate::metrics_recorder::label_from_path(&args.input),
        None,
        report.would_convert,
        input_bytes,
        output_bytes,
    );

    Ok(())
}
