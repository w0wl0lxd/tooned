// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned diff <file>` -- compare the original JSON representation with the
//! round-trip JSON obtained by encoding to TOON and decoding back.
//!
//! This is a verification helper: any divergence is a bug in the conversion
//! pipeline that `tooned` silently downgrades to `Passthrough` in normal
//! operation. `diff` surfaces it explicitly.

use std::path::PathBuf;

use clap::Args;
use similar::TextDiff;

#[derive(Debug, Args)]
pub struct DiffArgs {
    /// Input file to compare against its TOON round-trip.
    pub file: PathBuf,

    /// Number of context lines in the unified diff.
    #[arg(long, default_value = "3")]
    pub context: usize,
}

pub fn run(args: &DiffArgs) -> anyhow::Result<()> {
    let bytes = std::fs::read(&args.file)?;

    let opts = tooned_core::ConversionOptions::default();
    let toon_text = match tooned_core::maybe_tooned(&bytes, &opts) {
        Ok(tooned_core::Conversion::Toon { text, .. }) => text,
        Ok(tooned_core::Conversion::Passthrough { reason, .. }) => {
            eprintln!("tooned diff: input was not converted: {reason:?}");
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
    let roundtrip = tooned_core::decode_toon(toon_text.as_ref())?;

    crate::metrics_recorder::record_activity(crate::metrics_recorder::CliSurface::Diff, "diff");

    let left = sonic_rs::to_string_pretty(&original)?;
    let right = sonic_rs::to_string_pretty(&roundtrip)?;

    if left == right {
        println!("no diff");
        return Ok(());
    }

    let diff = TextDiff::from_lines(&left, &right);
    print!(
        "{}",
        diff.unified_diff()
            .context_radius(args.context)
            .header(&args.file.display().to_string(), "toon-roundtrip")
    );

    Ok(())
}
