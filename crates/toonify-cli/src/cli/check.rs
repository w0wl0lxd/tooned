//! `tooned check <file|-> [--precise]`
//!
//! Dry-run: prints doc type, shape class, byte-size comparison, convertible
//! y/n. Never writes converted output (no `maybe_tooned`/TOON-text call at
//! all -- only `tooned_core::inspect`, which by contract never computes or
//! returns TOON text).

use std::path::PathBuf;

use clap::Args;
use tooned_core::{ConversionOptions, inspect};

use crate::cli::io::read_input;

#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Input file, or `-` for stdin.
    pub input: PathBuf,

    /// Additionally report BPE-token-based savings (opt-in, FR-023).
    #[arg(long)]
    pub precise: bool,
}

// `Result` is kept (rather than `()`) to match every other subcommand's
// `run` signature uniformly dispatched from `main.rs`. Unlike `convert`,
// `check` is documented (`contracts/cli.md`) to exit 0 unconditionally -- "a
// 'not convertible' result is not a CLI error" -- with no I/O-error
// exception, so a read failure here must NOT hard-exit non-zero; it's
// reported on stdout/stderr and `run` still returns `Ok(())`.
#[allow(clippy::unnecessary_wraps)]
pub fn run(args: &CheckArgs) -> anyhow::Result<()> {
    let bytes = match read_input(&args.input) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("tooned: failed to read {}: {err}", args.input.display());
            println!("error: failed to read {}: {err}", args.input.display());
            println!("convertible: no");
            return Ok(());
        }
    };

    let opts = ConversionOptions { precise_tokens: args.precise, ..ConversionOptions::default() };
    let report = inspect(&bytes, &opts);

    let doc_type = match report.doc_type {
        Some(dt) => format!("{dt:?}"),
        None => "unknown".to_string(),
    };
    println!("doc type: {doc_type}");
    println!("shape: {:?}", report.shape);
    println!("input bytes: {}", report.input_bytes);
    if let (Some(json_bytes), Some(toon_bytes), Some(savings_pct)) =
        (report.json_bytes, report.toon_bytes, report.savings_pct)
    {
        println!("json bytes: {json_bytes}");
        println!("toon bytes: {toon_bytes}");
        println!("savings: {savings_pct:.1}%");
    } else {
        println!("json bytes: n/a");
        println!("toon bytes: n/a");
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

    Ok(())
}
