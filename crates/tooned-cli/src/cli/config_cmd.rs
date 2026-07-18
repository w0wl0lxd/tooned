// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned config validate [--config <path>]`
//!
//! Loads and validates a tooned configuration file. Exits 0 on a valid file,
//! non-zero with a parsing/loading error otherwise.

use std::path::PathBuf;

use clap::{Args, Subcommand};

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
    }
}
