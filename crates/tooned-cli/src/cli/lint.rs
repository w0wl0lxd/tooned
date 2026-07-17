// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned lint <file|->`
//!
//! Validate that a file is valid TOON, round-trips to JSON, and is free of
//! common anti-patterns. Exits 0 on a clean lint, non-zero with a diagnostic
//! on any problem.

use std::path::PathBuf;

use clap::Args;
use tooned_core::ToonedError;

use crate::cli::io::{BoundedRead, open_input, read_bounded};

#[derive(Debug, Args)]
pub struct LintArgs {
    /// Input file, or `-` for stdin.
    pub input: PathBuf,

    /// Maximum input size in bytes before rejection (default 2 MiB).
    #[arg(long = "max-bytes")]
    pub max_bytes: Option<u64>,

    /// Path to a tooned config file.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

pub fn run(args: &LintArgs) -> anyhow::Result<()> {
    let config = crate::config::Config::load(args.config.as_deref())?;
    let opts = config.conversion_options(None, args.max_bytes, None, None);

    let mut reader = open_input(&args.input).map_err(|err| {
        anyhow::anyhow!("tooned lint: failed to read {}: {err}", args.input.display())
    })?;

    let mut sink = std::io::sink();
    let bytes = match read_bounded(reader.as_mut(), opts.max_input_bytes, &mut sink) {
        Ok(BoundedRead::Fits(bytes)) => bytes,
        Ok(BoundedRead::Streamed { total_bytes }) => {
            anyhow::bail!(
                "tooned lint: input is too large ({total_bytes} bytes > {})",
                opts.max_input_bytes
            );
        }
        Err(err) => {
            anyhow::bail!("tooned lint: failed to read {}: {err}", args.input.display());
        }
    };

    let text = std::str::from_utf8(&bytes)
        .map_err(|_| anyhow::anyhow!("tooned lint: input is not valid UTF-8"))?;

    let value = match tooned_core::decode_toon(text) {
        Ok(value) => value,
        Err(ToonedError::InputTooLarge) => {
            anyhow::bail!("tooned lint: input exceeds the {} byte limit", opts.max_input_bytes);
        }
        Err(ToonedError::DecodeFailed(msg)) => {
            anyhow::bail!("tooned lint: not valid TOON: {msg}");
        }
        Err(err) => {
            anyhow::bail!("tooned lint: {err:?}");
        }
    };

    let encoded = tooned_toon::encode_toon(&value)
        .map_err(|err| anyhow::anyhow!("tooned lint: re-encode failed: {err:?}"))?;

    let round_trip = tooned_core::decode_toon(&encoded)
        .map_err(|err| anyhow::anyhow!("tooned lint: round-trip decode failed: {err:?}"))?;

    if round_trip != value {
        anyhow::bail!(
            "tooned lint: round-trip mismatch -- TOON encoding is not lossless for this value"
        );
    }

    let mut warnings = Vec::new();
    if let Some(array) = value.as_array() {
        if array.is_empty() {
            warnings.push("empty top-level array");
        } else if !array.iter().all(serde_json::Value::is_object) {
            warnings.push("top-level array contains non-object rows");
        } else if let Some(first) = array.first().and_then(serde_json::Value::as_object) {
            let keys: Vec<&str> = first.keys().map(String::as_str).collect();
            let inconsistent = array.iter().skip(1).any(|v| {
                v.as_object()
                    .is_none_or(|obj| obj.keys().map(String::as_str).collect::<Vec<_>>() != keys)
            });
            if inconsistent {
                warnings.push("top-level array rows have inconsistent key sets");
            }
        }
    } else if !value.is_object() {
        warnings.push("top-level value is neither an object nor a uniform array of objects");
    }

    if warnings.is_empty() {
        println!("ok: valid TOON and round-trips losslessly");
    } else {
        println!("ok: valid TOON and round-trips losslessly (with warnings)");
        for warning in warnings {
            eprintln!("warning: {warning}");
        }
    }
    Ok(())
}
