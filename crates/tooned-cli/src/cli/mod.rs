// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned` standalone CLI subcommands: `convert`, `check`, `pipe`, `wrap`,
//! `index`, `stats`. See `specs/001-adaptive-toon-conversion/contracts/cli.md`.

pub mod check;
pub mod convert;
pub mod diff;
pub mod index;
mod io;
pub mod pipe;
pub mod stats;
pub mod wrap;

/// `--format-hint` value for `convert`/`check`/`pipe`: forces the parser's
/// `DocType` rather than relying on content-sniffing, mirroring the MCP
/// tools' `format_hint` string parameter (`contracts/mcp-tools.md`'s
/// hint-first contract, FR-002) -- previously only reachable over MCP, with
/// no CLI equivalent at all (`--to toon`/`--to json` on `convert` forces
/// conversion *direction*, not the parser doctype, so it couldn't fix a
/// wrong doctype guess either).
#[derive(Debug, Clone, Copy, PartialEq, clap::ValueEnum)]
pub enum FormatHint {
    Json,
    Ndjson,
    Yaml,
    Toml,
    Csv,
    Tsv,
    Xml,
}

impl From<FormatHint> for tooned_core::DocType {
    fn from(hint: FormatHint) -> Self {
        match hint {
            FormatHint::Json => tooned_core::DocType::Json,
            FormatHint::Ndjson => tooned_core::DocType::NdJson,
            FormatHint::Yaml => tooned_core::DocType::Yaml,
            FormatHint::Toml => tooned_core::DocType::Toml,
            FormatHint::Csv => tooned_core::DocType::Csv,
            FormatHint::Tsv => tooned_core::DocType::Tsv,
            FormatHint::Xml => tooned_core::DocType::Xml,
        }
    }
}
