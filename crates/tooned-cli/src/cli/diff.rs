// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned diff <file>` -- compare the original JSON representation with the
//! round-trip JSON obtained by encoding to TOON and decoding back.
//!
//! This is a verification helper: any divergence is a bug in the conversion
//! pipeline that `tooned` silently downgrades to `Passthrough` in normal
//! operation. `diff` surfaces it explicitly.

use std::path::PathBuf;

use clap::Args;
use serde::Serialize;
use similar::TextDiff;

use crate::cli::io::resolve_input_path;

#[derive(Debug, Args)]
pub struct DiffArgs {
    /// Input file to compare against its TOON round-trip.
    pub file: PathBuf,

    /// Number of context lines in the unified diff.
    #[arg(long, default_value = "3")]
    pub context: usize,

    /// Emit the result as machine-readable JSON.
    #[arg(short = 'j', long)]
    pub json: bool,
}

#[derive(Serialize)]
struct DiffResult {
    equal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub fn run(args: &DiffArgs) -> anyhow::Result<()> {
    let file = resolve_input_path(&args.file)?;
    let bytes = std::fs::read(&file)?;

    let opts = tooned_core::ConversionOptions::default();
    let toon_text = match tooned_core::maybe_tooned(&bytes, &opts) {
        Ok(tooned_core::Conversion::Toon { text, .. }) => text,
        Ok(tooned_core::Conversion::Passthrough { reason, .. }) => {
            if args.json {
                println!(
                    "{}",
                    sonic_rs::to_string(&DiffResult {
                        equal: false,
                        diff: None,
                        error: Some(format!("input was not converted: {reason}")),
                    })?
                );
            } else {
                eprintln!("tooned diff: input was not converted: {reason}");
            }
            std::process::exit(2);
        }
        Err(err) => return Err(err.into()),
    };

    // Parse the original as a structured value via the same detect+parse step
    // the conversion pipeline uses, so the comparison is structural rather
    // than textual and works for JSON, NDJSON, YAML, TOML, CSV, TSV, and XML
    // originals (not binary MessagePack/CBOR/JSON5).
    let original: serde_json::Value = tooned_core::parse_to_value(&bytes, None)
        .map_err(|e| anyhow::anyhow!("tooned diff: cannot parse input as structured data: {e}"))?;
    let roundtrip = tooned_core::decode_toon(&toon_text)?;

    crate::metrics_recorder::record_activity(crate::metrics_recorder::CliSurface::Diff, "diff");

    let left = sonic_rs::to_string_pretty(&original)?;
    let right = sonic_rs::to_string_pretty(&roundtrip)?;

    if left == right {
        if args.json {
            println!(
                "{}",
                sonic_rs::to_string(&DiffResult { equal: true, diff: None, error: None })?
            );
        } else {
            println!("no diff");
        }
        return Ok(());
    }

    let diff = TextDiff::from_lines(&left, &right);
    let diff_text = diff
        .unified_diff()
        .context_radius(args.context)
        .header(&file.display().to_string(), "toon-roundtrip")
        .to_string();

    if args.json {
        println!(
            "{}",
            sonic_rs::to_string(&DiffResult { equal: false, diff: Some(diff_text), error: None })?
        );
    } else {
        print!("{diff_text}");
    }

    std::process::exit(3);
}
