// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned config <subcommand>`
//!
//! - `validate`: load and validate a tooned configuration file, surfacing
//!   parsing errors and unknown keys.
//! - `init`: write a commented starter configuration file.

use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::cli::io::write_output;

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Validate a tooned configuration file.
    Validate {
        /// Path to the configuration file to validate (default: discovered).
        #[arg(short = 'c', long)]
        config: Option<PathBuf>,
    },
    /// Write a commented starter tooned configuration file.
    Init {
        /// Path to write the starter config (default: stdout).
        #[arg(short = 'o', long, value_name = "PATH")]
        out: Option<PathBuf>,
    },
}

pub fn run(args: &ConfigArgs) -> anyhow::Result<()> {
    match &args.command {
        ConfigCommand::Validate { config } => {
            match crate::config::Config::load(config.as_deref()) {
                Ok(_) => {
                    println!("tooned config: valid");
                    Ok(())
                }
                Err(err) => Err(err),
            }
        }
        ConfigCommand::Init { out } => {
            let defaults = tooned_core::ConversionOptions::default();
            let text = format!(
                "# tooned configuration file\n\
                 # See `tooned config validate` to check this file.\n\
                 \n\
                 # Minimum percentage by which TOON must beat compact JSON\n\
                 # before it is used (default: {default_margin}).\n\
                 margin_pct = {default_margin}\n\
                 \n\
                 # Hard cap on input size in bytes before passthrough\n\
                 # (default: {default_max}).\n\
                 max_input_bytes = {default_max}\n\
                 \n\
                 # Default document-type hint: \"json\", \"ndjson\", \"yaml\",\n\
                 # \"toml\", \"csv\", \"tsv\", \"xml\", \"msgpack\",\n\
                 # \"cbor\", or \"json5\".\n\
                 # format_hint = \"json\"\n\
                 \n\
                 # Use BPE-token-based savings in `tooned check`/`token-savings`.\n\
                 # precise_tokens = false\n\
                 \n\
                 # Default watch-mode debounce in milliseconds.\n\
                 [watch]\n\
                 debounce_ms = {default_debounce}\n\
                 \n\
                 # Disable all local metrics recording.\n\
                 # metrics_disabled = false\n",
                default_margin = defaults.margin_pct,
                default_max = defaults.max_input_bytes,
                default_debounce = 1000,
            );
            write_output(out.as_deref(), text.as_bytes())
                .map_err(|e| anyhow::anyhow!("tooned config init: failed to write output: {e}"))
        }
    }
}
