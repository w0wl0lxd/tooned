// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned mcp serve`: the agent-agnostic Model Context Protocol server.
//! See `specs/001-adaptive-toon-conversion/contracts/mcp-tools.md`.

pub mod server;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct McpArgs {
    #[command(subcommand)]
    pub command: McpCommand,
}

#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Runs the MCP server over stdio.
    Serve,
}

pub fn run(args: &McpArgs) -> anyhow::Result<()> {
    match args.command {
        McpCommand::Serve => server::serve(),
    }
}
