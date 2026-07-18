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
    #[command(
        alias = "c",
        after_help = "Examples:\n  tooned convert data.json\n  tooned convert data.json --to toon --out data.toon\n  tooned convert data.toon --to json"
    )]
    Convert(cli::convert::ConvertArgs),
    /// Dry-run: prints doc type, shape class, byte-size comparison, convertible y/n.
    #[command(alias = "chk")]
    Check(cli::check::CheckArgs),
    /// stdin -> maybe_tooned -> stdout.
    #[command(
        alias = "p",
        after_help = "Examples:\n  curl -s https://api.example.com/users | tooned pipe\n  cat data.json | tooned pipe --margin 5"
    )]
    Pipe(cli::pipe::PipeArgs),
    /// Runs a wrapped command and adaptively converts its captured stdout.
    #[command(
        alias = "w",
        after_help = "Examples:\n  tooned wrap -- gh pr list --json number,title,author\n  tooned wrap -- cat data.json"
    )]
    Wrap(cli::wrap::WrapArgs),
    /// Full scan / sync / status / show against the `.tooned/` project index.
    #[command(
        alias = "i",
        after_help = "Examples:\n  tooned index .\n  tooned index sync .\n  tooned index status\n  tooned index show data.json"
    )]
    Index(cli::index::IndexArgs),
    /// Ranked savings-opportunity report from the index.
    #[command(alias = "s")]
    Stats(cli::stats::StatsArgs),
    /// Compare the original JSON with the TOON round-trip.
    #[command(alias = "d")]
    Diff(cli::diff::DiffArgs),
    /// Validate a TOON file: parse, round-trip, and anti-pattern checks.
    #[command(alias = "l")]
    Lint(cli::lint::LintArgs),
    /// Agent hook install/uninstall/status/doctor (Claude Code, Codex, Devin, Droid, OpenCode, Kilo, Pi).
    #[command(alias = "h")]
    Hook(hooks::HookArgs),
    /// Model Context Protocol server.
    #[command(alias = "m")]
    Mcp(mcp::McpArgs),
    /// GitHub/Codex-style token-savings heatmap.
    #[command(
        alias = "hm",
        after_help = "Examples:\n  tooned heatmap\n  tooned heatmap --global --metric bytes\n  tooned heatmap --since 2024-01-01"
    )]
    Heatmap(cli::heatmap::HeatmapArgs),
    /// Inspect the local token-savings metrics ledger.
    #[command(alias = "met")]
    Metrics(cli::metrics::MetricsArgs),
    /// Interactive ratatui metrics dashboard.
    #[command(
        alias = "db",
        after_help = "Examples:\n  tooned dashboard\n  tooned dashboard --global"
    )]
    Dashboard(cli::dashboard::DashboardArgs),
    /// Generate shell completion scripts (bash, zsh, fish, nushell, elvish, powershell).
    #[command(alias = "comp")]
    Completions {
        /// Target shell.
        #[arg(long, value_name = "SHELL")]
        shell: Shell,
    },
    /// Generate the man page (roff).
    Man,
    /// Validate tooned configuration files.
    #[command(
        alias = "cfg",
        after_help = "Examples:\n  tooned config validate\n  tooned config validate --config tooned.toml"
    )]
    Config(cli::config_cmd::ConfigArgs),
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
        Command::Lint(args) => cli::lint::run(args),
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
        Command::Config(args) => cli::config_cmd::run(args),
    }
}
