// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned stats [path] [--top N]`
//!
//! Ranked savings-opportunity report from the index (FR-022, T060). Exit
//! codes: 0 success, 1 no existing index.

use std::path::PathBuf;

use clap::Args;
use serde::Serialize;

#[derive(Debug, Args)]
pub struct StatsArgs {
    /// Project path (default: current directory).
    pub path: Option<PathBuf>,

    /// Limit results to the top N entries by savings percentage.
    #[arg(short = 'n', long)]
    pub top: Option<u32>,

    /// Emit the report as a JSON array instead of human-readable text.
    #[arg(short = 'j', long)]
    pub json: bool,
}

#[derive(Serialize)]
struct StatsEntry<'a> {
    path: &'a str,
    json_bytes: i64,
    toon_bytes: i64,
    savings_pct: f64,
}

pub fn run(args: &StatsArgs) -> anyhow::Result<()> {
    let root = match &args.path {
        Some(p) => p.clone(),
        None => PathBuf::from("."),
    };

    match tooned_index::stats(&root, args.top) {
        Ok(rows) => {
            if args.json {
                let entries: Vec<StatsEntry> = rows
                    .iter()
                    .map(|row| StatsEntry {
                        path: &row.path,
                        json_bytes: row.json_bytes,
                        toon_bytes: row.toon_bytes,
                        savings_pct: row.savings_pct,
                    })
                    .collect();
                println!("{}", sonic_rs::to_string(&entries)?);
                return Ok(());
            }

            if rows.is_empty() {
                println!("No conversion data in the index yet. Run `tooned index` first.");
                return Ok(());
            }
            for row in &rows {
                println!(
                    "{:>6.1}%  {}  ({} -> {} bytes)",
                    row.savings_pct, row.path, row.json_bytes, row.toon_bytes
                );
            }
            Ok(())
        }
        Err(tooned_index::IndexError::NoIndex(path)) => {
            eprintln!(
                "tooned stats: no existing index at {}; run `tooned index` first",
                tooned_index::index_db_path(&path).display()
            );
            std::process::exit(1);
        }
        Err(other) => Err(other.into()),
    }
}
