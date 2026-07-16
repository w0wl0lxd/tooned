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

    // `diff` is most useful for JSON today: the original is parsed so the
    // comparison is structural, not textual. Support for YAML/TOML/XML/CSV
    // originals can be added once `tooned-core` exposes a shared parser.
    let original: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("tooned diff currently supports JSON inputs: {e}"))?;
    let roundtrip = tooned_core::decode_toon(&toon_text)?;

    let left = serde_json::to_string_pretty(&original)?;
    let right = serde_json::to_string_pretty(&roundtrip)?;

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
