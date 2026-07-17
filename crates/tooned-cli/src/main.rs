// SPDX-License-Identifier: AGPL-3.0-only

//! tooned CLI entrypoint.
//!
//! Scaffold only: subcommands (`convert`, `check`, `pipe`, `wrap`, `index`,
//! `stats`, `diff`, `hook`, `heatmap`, `metrics`, `mcp`) are implemented following the spec-kit pipeline
//! (`specs/`), not directly in this initial commit. See
//! `specs/001-adaptive-toon-conversion/contracts/cli.md` for the exact
//! command surface every variant below mirrors.

mod cli;
mod config;
mod hooks;
mod mcp;
mod metrics_recorder;
mod tui;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

#[derive(Parser)]
#[command(
    name = "tooned",
    version,
    about = "Transparent TOON re-encoding for AI coding agent tool-call context"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// One-shot conversion; stdout by default.
    Convert(cli::convert::ConvertArgs),
    /// Dry-run: prints doc type, shape class, byte-size comparison, convertible y/n.
    Check(cli::check::CheckArgs),
    /// stdin -> maybe_tooned -> stdout.
    Pipe(cli::pipe::PipeArgs),
    /// Runs a wrapped command and adaptively converts its captured stdout.
    Wrap(cli::wrap::WrapArgs),
    /// Full scan / sync / status / show against the `.tooned/` project index.
    Index(cli::index::IndexArgs),
    /// Ranked savings-opportunity report from the index.
    Stats(cli::stats::StatsArgs),
    /// Compare the original JSON with the TOON round-trip.
    Diff(cli::diff::DiffArgs),
    /// Agent hook install/uninstall/status/doctor (Claude Code, Codex, Devin, Droid, OpenCode, Kilo, Pi).
    Hook(hooks::HookArgs),
    /// Model Context Protocol server.
    Mcp(mcp::McpArgs),
    /// GitHub/Codex-style token-savings heatmap.
    Heatmap(cli::heatmap::HeatmapArgs),
    /// Inspect the local token-savings metrics ledger.
    Metrics(cli::metrics::MetricsArgs),
    /// Interactive ratatui metrics dashboard.
    Dashboard(cli::dashboard::DashboardArgs),
    /// Generate shell completion scripts (bash, zsh, fish, nushell, elvish, powershell).
    Completions {
        /// Target shell.
        #[arg(long, value_name = "SHELL")]
        shell: Shell,
    },
    /// Generate the man page (roff).
    Man,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Command::Convert(args) => cli::convert::run(args),
        Command::Check(args) => cli::check::run(args),
        Command::Pipe(args) => cli::pipe::run(args),
        Command::Wrap(args) => cli::wrap::run(args),
        Command::Index(args) => cli::index::run(args),
        Command::Stats(args) => cli::stats::run(args),
        Command::Diff(args) => cli::diff::run(args),
        Command::Hook(args) => {
            hooks::run(args);
            Ok(())
        }
        Command::Mcp(args) => mcp::run(args),
        Command::Heatmap(args) => cli::heatmap::run(args),
        Command::Metrics(args) => cli::metrics::run(args),
        Command::Dashboard(args) => cli::dashboard::run(args),
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(*shell, &mut cmd, name, &mut std::io::stdout());
            Ok(())
        }
        Command::Man => {
            clap_mangen::Man::new(Cli::command()).render(&mut std::io::stdout())?;
            Ok(())
        }
    }
}
