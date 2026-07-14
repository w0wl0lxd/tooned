//! `tooned` standalone CLI subcommands: `convert`, `check`, `pipe`, `wrap`,
//! `index`, `stats`. See `specs/001-adaptive-toon-conversion/contracts/cli.md`.

pub mod check;
pub mod convert;
pub mod index;
mod io;
pub mod pipe;
pub mod stats;
pub mod wrap;
