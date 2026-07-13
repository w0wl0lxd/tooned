//! tooned CLI entrypoint.
//!
//! Scaffold only: subcommands (`convert`, `check`, `pipe`, `wrap`, `index`,
//! `stats`, `hook`, `mcp`) are implemented following the spec-kit pipeline
//! (`specs/`), not directly in this initial commit.

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "tooned",
    version,
    about = "Transparent TOON re-encoding for AI coding agent tool-call context"
)]
struct Cli;

fn main() {
    let _cli = Cli::parse();
}
