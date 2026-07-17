// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned dashboard` -- interactive ratatui dashboard for the metrics ledger.

use clap::Args;

use crate::cli::metrics::{MetricsWindow, ledger_path};

/// `tooned dashboard`
#[derive(Debug, Args)]
pub struct DashboardArgs {
    /// Read from the user-global ledger instead of the project ledger.
    #[arg(long)]
    pub global: bool,

    #[command(flatten)]
    pub window: MetricsWindow,
}

pub fn run(args: &DashboardArgs) -> anyhow::Result<()> {
    let path = ledger_path(args.global)?;
    crate::tui::run(&path, &args.window, args.global)
}
