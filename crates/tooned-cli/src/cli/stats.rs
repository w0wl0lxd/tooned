// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned stats [path] [--top N] [--sort-by savings|count|recency]`
//!
//! Ranked savings-opportunity report from the index (FR-022, T060). Exit
//! codes: 0 success, 1 no existing index.

use std::path::PathBuf;

use clap::{Args, ValueEnum};
use serde::Serialize;

use tooned_index::{DocTypeFilter, IndexFilter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StatsSortBy {
    Savings,
    Count,
    Recency,
}

impl From<StatsSortBy> for tooned_index::SortBy {
    fn from(sort: StatsSortBy) -> Self {
        match sort {
            StatsSortBy::Savings => tooned_index::SortBy::Savings,
            StatsSortBy::Count => tooned_index::SortBy::Count,
            StatsSortBy::Recency => tooned_index::SortBy::Recency,
        }
    }
}

#[derive(Debug, Args)]
pub struct StatsArgs {
    /// Project path (default: current directory).
    pub path: Option<PathBuf>,

    /// Limit results to the top N entries.
    #[arg(short = 'n', long)]
    pub top: Option<u32>,

    /// Rank results by savings, conversion count, or recency.
    #[arg(long = "sort-by")]
    pub sort_by: Option<StatsSortBy>,

    /// Emit the report as a JSON array instead of human-readable text.
    #[arg(short = 'j', long)]
    pub json: bool,

    /// Only include files of this document type (json, ndjson, yaml, toml, csv, tsv, xml, msgpack, cbor, json5, bin).
    #[arg(long, value_name = "TYPE")]
    pub type_filter: Option<String>,

    /// Exclude paths matching these gitignore-style globs (repeatable).
    #[arg(long, value_name = "GLOB")]
    pub exclude: Vec<String>,
}

#[derive(Serialize)]
struct StatsEntry<'a> {
    path: &'a str,
    json_bytes: i64,
    toon_bytes: i64,
    savings_pct: f64,
    score: f64,
}

pub fn run(args: &StatsArgs) -> anyhow::Result<()> {
    let root = match &args.path {
        Some(p) => p.clone(),
        None => PathBuf::from("."),
    };

    let type_filter = match &args.type_filter {
        Some(s) => {
            let parsed = DocTypeFilter::parse(s)
                .ok_or_else(|| anyhow::anyhow!("invalid type filter: {s}"))?;
            Some(parsed)
        }
        None => None,
    };
    let filter =
        IndexFilter { type_filter, excludes: args.exclude.clone(), respect_gitignore: true };

    let sort_by = args.sort_by.map_or(tooned_index::SortBy::Savings, Into::into);
    match tooned_index::stats_sorted(&root, args.top, sort_by, &filter) {
        Ok(rows) => {
            if args.json {
                let entries: Vec<StatsEntry> = rows
                    .iter()
                    .map(|row| StatsEntry {
                        path: &row.path,
                        json_bytes: row.json_bytes,
                        toon_bytes: row.toon_bytes,
                        savings_pct: row.savings_pct,
                        score: row.score,
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
