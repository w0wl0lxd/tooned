//! `tooned index [path]`, `index sync`, `index status`, `index show <file>`
//!
//! Full scan + classify + cache into `.tooned/index.db` (default: cwd), and
//! its incremental/status/show variants (T059). Exit codes per
//! `specs/001-adaptive-toon-conversion/contracts/cli.md`:
//! `index`: 0 success, 2 path not found.
//! `index sync`: 0 success, 1 no existing index.
//! `index status`: 0 always.
//! `index show <file>`: 0 success, 2 file not indexed.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct IndexArgs {
    /// Project path (default: current directory). Ignored when a subcommand is given.
    pub path: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<IndexSubcommand>,
}

#[derive(Debug, Subcommand)]
pub enum IndexSubcommand {
    /// Incremental: stat-first, re-hash/re-classify only on real change.
    Sync { path: Option<PathBuf> },
    /// Reports index existence, file count, last scan time.
    Status { path: Option<PathBuf> },
    /// Reports the indexed record for one file.
    Show { file: PathBuf },
}

fn resolve_project_root(path: Option<&PathBuf>) -> PathBuf {
    match path {
        Some(p) => p.clone(),
        None => PathBuf::from("."),
    }
}

pub fn run(args: &IndexArgs) -> anyhow::Result<()> {
    match &args.command {
        None => run_scan(&resolve_project_root(args.path.as_ref())),
        Some(IndexSubcommand::Sync { path }) => run_sync(&resolve_project_root(path.as_ref())),
        Some(IndexSubcommand::Status { path }) => run_status(&resolve_project_root(path.as_ref())),
        Some(IndexSubcommand::Show { file }) => run_show(file),
    }
}

fn run_scan(root: &Path) -> anyhow::Result<()> {
    if !root.is_dir() {
        eprintln!("path not found: {}", root.display());
        std::process::exit(2);
    }

    let summary = tooned_index::scan_full(root)?;
    println!(
        "Indexed {} file(s) ({} classified) at {}",
        summary.files_scanned,
        summary.files_classified,
        tooned_index::index_db_path(root).display()
    );
    Ok(())
}

fn run_sync(root: &Path) -> anyhow::Result<()> {
    match tooned_index::sync(root) {
        Ok(summary) => {
            println!(
                "Synced {}: {} added, {} updated, {} unchanged, {} removed",
                root.display(),
                summary.added,
                summary.updated,
                summary.unchanged,
                summary.removed
            );
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

fn run_status(root: &Path) -> anyhow::Result<()> {
    let status = tooned_index::status(root)?;
    if !status.exists {
        println!("No index yet at {}. Run `tooned index` to create one.", root.display());
        return Ok(());
    }

    match status.last_scanned_at {
        Some(last_scanned_at) => println!(
            "Index at {}: {} file(s), last scanned at unix time {last_scanned_at}",
            tooned_index::index_db_path(root).display(),
            status.file_count
        ),
        None => println!(
            "Index at {}: {} file(s), never scanned",
            tooned_index::index_db_path(root).display(),
            status.file_count
        ),
    }
    Ok(())
}

fn run_show(file: &Path) -> anyhow::Result<()> {
    // `index show <file>` takes no project-root argument per the CLI
    // contract -- the project root is always the current directory, and
    // `file` is looked up relative to it (matching the path format
    // `scan_full`/`sync` store: relative to the scanned root).
    let root = PathBuf::from(".");
    let rel = match file.strip_prefix(&root) {
        Ok(stripped) => stripped,
        Err(_) => file,
    };
    let Some(rel_str) = rel.to_str() else {
        eprintln!("file path is not valid UTF-8: {}", file.display());
        std::process::exit(2);
    };

    match tooned_index::show_file(&root, rel_str) {
        Ok(detail) => {
            println!("{}", detail.file.path);
            println!("  size_bytes:   {}", detail.file.size_bytes);
            println!("  content_hash: {}", detail.file.content_hash);
            match &detail.file.doc_type {
                Some(doc_type) => println!("  doc_type:     {doc_type}"),
                None => println!("  doc_type:     (not a recognized doctype)"),
            }
            for shape in &detail.shapes {
                println!("  shape:        {}", shape.shape_class);
            }
            for conversion in &detail.conversions {
                println!(
                    "  conversion:   {} -> {} bytes ({:.1}% savings)",
                    conversion.json_bytes, conversion.toon_bytes, conversion.savings_pct
                );
            }
            Ok(())
        }
        Err(tooned_index::IndexError::NoIndex(path)) => {
            eprintln!("no index found at {}; run `tooned index` first", path.display());
            std::process::exit(2);
        }
        Err(tooned_index::IndexError::FileNotIndexed(path)) => {
            eprintln!("file not indexed: {path}");
            std::process::exit(2);
        }
        Err(other) => Err(other.into()),
    }
}
