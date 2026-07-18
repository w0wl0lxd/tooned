// SPDX-License-Identifier: AGPL-3.0-only

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
use serde::Serialize;

use crate::config::Config;
use tooned_index::{DocTypeFilter, IndexFilter};

#[derive(Debug, Args)]
pub struct IndexArgs {
    /// Project path (default: current directory). Ignored when a subcommand is given.
    pub path: Option<PathBuf>,

    /// Only include files of this document type (json, ndjson, yaml, toml, csv, tsv, xml, msgpack, cbor, json5, bin).
    #[arg(short = 't', long, value_name = "TYPE")]
    pub type_filter: Option<String>,

    /// Exclude paths matching these gitignore-style globs (repeatable).
    #[arg(short = 'x', long, value_name = "GLOB")]
    pub exclude: Vec<String>,

    /// Emit the result as machine-readable JSON.
    #[arg(short = 'j', long)]
    pub json: bool,

    /// Show what would be done without modifying the index.
    #[arg(long)]
    pub dry_run: bool,

    /// Also index local `path`-type flake inputs discovered in `flake.lock`.
    #[arg(long)]
    pub include_flake_inputs: bool,

    #[command(subcommand)]
    pub command: Option<IndexSubcommand>,
}

#[derive(Debug, Subcommand)]
pub enum IndexSubcommand {
    /// Incremental: stat-first, re-hash/re-classify only on real change.
    Sync {
        path: Option<PathBuf>,
        /// Only include files of this document type (json, ndjson, yaml, toml, csv, tsv, xml, msgpack, cbor, json5, bin).
        #[arg(short = 't', long, value_name = "TYPE")]
        type_filter: Option<String>,
        /// Exclude paths matching these gitignore-style globs (repeatable).
        #[arg(short = 'x', long, value_name = "GLOB")]
        exclude: Vec<String>,
        /// Emit the result as machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
        /// Show what would be synced without modifying the index.
        #[arg(long)]
        dry_run: bool,
        /// Also index local `path`-type flake inputs discovered in `flake.lock`.
        #[arg(long)]
        include_flake_inputs: bool,
    },
    /// Reports index existence, file count, last scan time.
    Status {
        path: Option<PathBuf>,
        /// Emit the result as machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },
    /// Reports the indexed record for one file.
    Show {
        file: PathBuf,
        /// Emit the result as machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },
    /// Checkpoint the SQLite WAL and truncate the `-wal` file.
    Compact {
        path: Option<PathBuf>,
        /// Emit the result as machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
        /// Show what would be compacted without modifying the index.
        #[arg(long)]
        dry_run: bool,
    },
    /// Watch `project_root` and run `index sync` on debounced filesystem
    /// events.
    Watch {
        path: Option<PathBuf>,
        /// Quiet period in milliseconds before a change triggers a sync.
        #[arg(short = 'd', long)]
        debounce_ms: Option<u64>,
        /// Only include files of this document type (json, ndjson, yaml, toml, csv, tsv, xml, msgpack, cbor, json5, bin).
        #[arg(short = 't', long, value_name = "TYPE")]
        type_filter: Option<String>,
        /// Exclude paths matching these gitignore-style globs (repeatable).
        #[arg(short = 'x', long, value_name = "GLOB")]
        exclude: Vec<String>,
        /// Also index local `path`-type flake inputs discovered in `flake.lock`.
        #[arg(long)]
        include_flake_inputs: bool,
    },
}

fn resolve_project_root(path: Option<&PathBuf>) -> PathBuf {
    let start = match path {
        Some(p) => p.clone(),
        None => PathBuf::from("."),
    };
    tooned_core::project_root(&start)
}

#[derive(Serialize)]
struct ScanJson {
    files_scanned: usize,
    files_classified: usize,
    index_path: String,
}

#[derive(Serialize)]
struct SyncJson {
    added: usize,
    updated: usize,
    unchanged: usize,
    removed: usize,
}

#[derive(Serialize)]
struct StatusJson {
    exists: bool,
    file_count: i64,
    last_scanned_at: Option<i64>,
}

#[derive(Serialize)]
struct ShowFileJson<'a> {
    path: &'a str,
    size_bytes: i64,
    content_hash: &'a str,
    doc_type: Option<&'a str>,
    shapes: Vec<ShapeJson<'a>>,
    conversions: Vec<ConversionJson>,
}

#[derive(Serialize)]
struct ShapeJson<'a> {
    json_pointer: &'a str,
    shape_class: &'a str,
    uniformity_pct: Option<f64>,
    sampled_count: Option<i64>,
}

#[derive(Serialize)]
struct ConversionJson {
    json_bytes: i64,
    toon_bytes: i64,
    savings_pct: f64,
}

#[derive(Serialize)]
struct CompactJson {
    compacted: bool,
    index_path: String,
}

fn build_filter(type_filter: Option<&String>, exclude: &[String]) -> anyhow::Result<IndexFilter> {
    let type_filter = match type_filter {
        Some(s) => {
            let parsed = DocTypeFilter::parse(s)
                .ok_or_else(|| anyhow::anyhow!("invalid type filter: {s}"))?;
            Some(parsed)
        }
        None => None,
    };
    Ok(IndexFilter { type_filter, excludes: exclude.to_vec() })
}

pub fn run(args: &IndexArgs) -> anyhow::Result<()> {
    match &args.command {
        None => {
            let filter = build_filter(args.type_filter.as_ref(), &args.exclude)?;
            run_scan(
                &resolve_project_root(args.path.as_ref()),
                &filter,
                args.json,
                args.dry_run,
                args.include_flake_inputs,
            )
        }
        Some(IndexSubcommand::Sync {
            path,
            type_filter,
            exclude,
            json,
            dry_run,
            include_flake_inputs,
        }) => {
            let filter = build_filter(type_filter.as_ref(), exclude)?;
            run_sync(
                &resolve_project_root(path.as_ref()),
                &filter,
                *json,
                *dry_run,
                *include_flake_inputs,
            )
        }
        Some(IndexSubcommand::Status { path, json }) => {
            run_status(&resolve_project_root(path.as_ref()), *json)
        }
        Some(IndexSubcommand::Show { file, json }) => run_show(file, *json),
        Some(IndexSubcommand::Compact { path, json, dry_run }) => {
            run_compact(&resolve_project_root(path.as_ref()), *json, *dry_run)
        }
        Some(IndexSubcommand::Watch {
            path,
            debounce_ms,
            type_filter,
            exclude,
            include_flake_inputs,
        }) => {
            let config = Config::load(None)?;
            let configured_debounce = config.watch.as_ref().and_then(|w| w.debounce_ms);
            // `clippy::disallowed_methods` forbids `unwrap_or` (silent default),
            // and the config-file fallback means the default isn't a simple
            // literal here, so spell it out explicitly.
            #[allow(clippy::manual_unwrap_or)]
            let debounce = match debounce_ms.or(configured_debounce) {
                Some(d) => d,
                None => 1000,
            };
            let filter = build_filter(type_filter.as_ref(), exclude)?;
            if *include_flake_inputs {
                eprintln!(
                    "tooned index watch: --include-flake-inputs is not yet supported for watch mode"
                );
            }
            Ok(tooned_index::watch(&resolve_project_root(path.as_ref()), debounce, &filter)?)
        }
    }
}

fn run_scan(
    root: &Path,
    filter: &IndexFilter,
    json: bool,
    dry_run: bool,
    include_flake_inputs: bool,
) -> anyhow::Result<()> {
    if !root.is_dir() {
        eprintln!("tooned index: path not found: {}", root.display());
        std::process::exit(2);
    }

    if dry_run {
        if json {
            println!(
                "{}",
                sonic_rs::to_string(&ScanJson {
                    files_scanned: 0,
                    files_classified: 0,
                    index_path: tooned_index::index_db_path(root).display().to_string(),
                })?
            );
        } else {
            println!(
                "Dry run: would scan {} and write index to {}",
                root.display(),
                tooned_index::index_db_path(root).display()
            );
        }
        return Ok(());
    }

    let mut summary = tooned_index::scan_full(root, filter)?;
    if include_flake_inputs {
        for input in tooned_index::flake_inputs(root) {
            if input.is_dir() {
                match tooned_index::scan_full(&input, filter) {
                    Ok(s) => {
                        summary.files_scanned += s.files_scanned;
                        summary.files_classified += s.files_classified;
                    }
                    Err(err) => {
                        eprintln!("tooned index: skipping flake input {}: {err}", input.display());
                    }
                }
            }
        }
    }
    if json {
        println!(
            "{}",
            sonic_rs::to_string(&ScanJson {
                files_scanned: summary.files_scanned,
                files_classified: summary.files_classified,
                index_path: tooned_index::index_db_path(root).display().to_string(),
            })?
        );
    } else {
        println!(
            "Indexed {} file(s) ({} classified) at {}",
            summary.files_scanned,
            summary.files_classified,
            tooned_index::index_db_path(root).display()
        );
    }
    crate::metrics_recorder::record_activity(crate::metrics_recorder::CliSurface::Index, "scan");
    Ok(())
}

fn run_sync(
    root: &Path,
    filter: &IndexFilter,
    json: bool,
    dry_run: bool,
    include_flake_inputs: bool,
) -> anyhow::Result<()> {
    if dry_run {
        if json {
            println!(
                "{}",
                sonic_rs::to_string(&SyncJson { added: 0, updated: 0, unchanged: 0, removed: 0 })?
            );
        } else {
            println!(
                "Dry run: would sync index at {}",
                tooned_index::index_db_path(root).display()
            );
        }
        return Ok(());
    }
    let mut summary = match tooned_index::sync(root, filter) {
        Ok(summary) => summary,
        Err(tooned_index::IndexError::NoIndex(path)) => {
            eprintln!(
                "tooned index sync: no existing index at {}; run `tooned index` first",
                tooned_index::index_db_path(&path).display()
            );
            std::process::exit(1);
        }
        Err(other) => return Err(other.into()),
    };

    if include_flake_inputs {
        for input in tooned_index::flake_inputs(root) {
            if !input.is_dir() {
                continue;
            }
            match tooned_index::sync(&input, filter) {
                Ok(s) => {
                    summary.added += s.added;
                    summary.updated += s.updated;
                    summary.unchanged += s.unchanged;
                    summary.removed += s.removed;
                }
                Err(tooned_index::IndexError::NoIndex(_)) => {
                    match tooned_index::scan_full(&input, filter) {
                        Ok(s) => {
                            summary.added += s.files_scanned;
                        }
                        Err(err) => {
                            eprintln!(
                                "tooned index sync: skipping flake input {}: {err}",
                                input.display()
                            );
                        }
                    }
                }
                Err(err) => {
                    eprintln!("tooned index sync: skipping flake input {}: {err}", input.display());
                }
            }
        }
    }

    if json {
        println!(
            "{}",
            sonic_rs::to_string(&SyncJson {
                added: summary.added,
                updated: summary.updated,
                unchanged: summary.unchanged,
                removed: summary.removed,
            })?
        );
    } else {
        println!(
            "Synced {}: {} added, {} updated, {} unchanged, {} removed",
            root.display(),
            summary.added,
            summary.updated,
            summary.unchanged,
            summary.removed
        );
    }
    crate::metrics_recorder::record_activity(crate::metrics_recorder::CliSurface::Index, "sync");
    Ok(())
}

fn run_status(root: &Path, json: bool) -> anyhow::Result<()> {
    let status = tooned_index::status(root)?;
    if json {
        println!(
            "{}",
            sonic_rs::to_string(&StatusJson {
                exists: status.exists,
                file_count: status.file_count,
                last_scanned_at: status.last_scanned_at,
            })?
        );
        return Ok(());
    }
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

fn run_show(file: &Path, json: bool) -> anyhow::Result<()> {
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
        eprintln!("tooned index show: file path is not valid UTF-8: {}", file.display());
        std::process::exit(2);
    };

    match tooned_index::show_file(&root, rel_str) {
        Ok(detail) => {
            if json {
                let shapes: Vec<ShapeJson> = detail
                    .shapes
                    .iter()
                    .map(|s| ShapeJson {
                        json_pointer: &s.json_pointer,
                        shape_class: &s.shape_class,
                        uniformity_pct: s.uniformity_pct,
                        sampled_count: s.sampled_count,
                    })
                    .collect();
                let conversions: Vec<ConversionJson> = detail
                    .conversions
                    .iter()
                    .map(|c| ConversionJson {
                        json_bytes: c.json_bytes,
                        toon_bytes: c.toon_bytes,
                        savings_pct: c.savings_pct,
                    })
                    .collect();
                let out = ShowFileJson {
                    path: &detail.file.path,
                    size_bytes: detail.file.size_bytes,
                    content_hash: &detail.file.content_hash,
                    doc_type: detail.file.doc_type.as_deref(),
                    shapes,
                    conversions,
                };
                println!("{}", sonic_rs::to_string(&out)?);
                return Ok(());
            }
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
            eprintln!(
                "tooned index show: no index found at {}; run `tooned index` first",
                path.display()
            );
            std::process::exit(2);
        }
        Err(tooned_index::IndexError::FileNotIndexed(path)) => {
            eprintln!("tooned index show: file not indexed: {path}");
            std::process::exit(2);
        }
        Err(other) => Err(other.into()),
    }
}

fn run_compact(root: &Path, json: bool, dry_run: bool) -> anyhow::Result<()> {
    if dry_run {
        if json {
            println!(
                "{}",
                sonic_rs::to_string(&CompactJson {
                    compacted: false,
                    index_path: tooned_index::index_db_path(root).display().to_string(),
                })?
            );
        } else {
            println!("Dry run: would compact {}", tooned_index::index_db_path(root).display());
        }
        return Ok(());
    }
    match tooned_index::compact(root) {
        Ok(()) => {
            if json {
                println!(
                    "{}",
                    sonic_rs::to_string(&CompactJson {
                        compacted: true,
                        index_path: tooned_index::index_db_path(root).display().to_string(),
                    })?
                );
            } else {
                println!("Compacted {}", tooned_index::index_db_path(root).display());
            }
            Ok(())
        }
        Err(tooned_index::IndexError::NoIndex(path)) => {
            eprintln!(
                "tooned index compact: no existing index at {}; run `tooned index` first",
                tooned_index::index_db_path(&path).display()
            );
            std::process::exit(1);
        }
        Err(other) => Err(other.into()),
    }
}
