//! `tooned stats [path] [--top N]`
//!
//! Ranked savings-opportunity report from the index (FR-022, T060). Exit
//! codes: 0 success, 1 no existing index.

use std::path::PathBuf;

use clap::Args;

#[derive(Debug, Args)]
pub struct StatsArgs {
    /// Project path (default: current directory).
    pub path: Option<PathBuf>,

    /// Limit results to the top N entries by savings percentage.
    #[arg(long)]
    pub top: Option<u32>,
}

pub fn run(args: &StatsArgs) -> anyhow::Result<()> {
    let root = match &args.path {
        Some(p) => p.clone(),
        None => PathBuf::from("."),
    };

    match tooned_index::stats(&root, args.top) {
        Ok(rows) => {
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
                "no existing index at {}; run `tooned index` first",
                tooned_index::index_db_path(&path).display()
            );
            std::process::exit(1);
        }
        Err(other) => Err(other.into()),
    }
}
