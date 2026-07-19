// SPDX-License-Identifier: AGPL-3.0-only

//! Full directory scan (T056): walk via the `ignore` crate (respects
//! `.gitignore`), blake3 content fingerprinting, doctype detection + shape
//! classification via `tooned_core::inspect`, persisted into
//! `files`/`shapes`/`conversions`.

use std::fs::File;
use std::path::Path;

use rusqlite::Connection;
use tooned_core::{ConversionOptions, DocType, InspectReport, ShapeClass};

use crate::IndexError;
use crate::filter::IndexFilter;
use crate::gitignore;
use crate::schema::{self, file_mtime_unix, now_unix, saturating_i64};

/// Hard cap on how many directory-walk entries a single `scan_full`/`sync`
/// run will ever visit, checked as the walker yields each entry (before any
/// per-file work happens). `path`/`project_root` is either human-typed (CLI
/// `tooned index`) or, via the MCP `tooned_index_build`/`tooned_index_
/// refresh`/`tooned_stats` tools, unrestricted and client-supplied with no
/// other validation -- an agent acting on attacker-influenced content could
/// point it at something far larger than a real project (a home directory,
/// say). Well above any real project's file count (the perf test suite
/// already exercises a full scan+sync cycle over 1,200 files end-to-end),
/// but still small enough to bound a single walk's resource cost rather
/// than leaving it fully unbounded.
const MAX_SCAN_ENTRIES: usize = 50_000;

/// Returns `Err(IndexError::ScanTooLarge)` once `visited` exceeds
/// [`MAX_SCAN_ENTRIES`]. Callers check this on every entry the walker
/// yields (not just ones that turn out to be regular files), so the cap
/// bounds the raw enumeration cost too, not only the count of files
/// actually persisted.
pub(crate) fn enforce_walk_cap(visited: usize) -> Result<(), IndexError> {
    if visited > MAX_SCAN_ENTRIES {
        return Err(IndexError::ScanTooLarge(MAX_SCAN_ENTRIES));
    }
    Ok(())
}

/// Result of a full [`scan_full`] run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanSummary {
    /// Every file the walker visited and persisted a `files` row for.
    pub files_scanned: usize,
    /// The subset of `files_scanned` that were a recognized doctype (i.e.
    /// got a `shapes`/`conversions` row too).
    pub files_classified: usize,
}

/// Full scan of `project_root`: walks the tree (respecting `.gitignore` via
/// the `ignore` crate), fingerprints and classifies every regular file
/// found, and persists the result into `.tooned/index.db`. On first
/// creation, also appends `.tooned/` to the project's `.gitignore`
/// (FR-020).
pub fn scan_full(project_root: &Path, filter: &IndexFilter) -> Result<ScanSummary, IndexError> {
    // Idempotent regardless of call order: appending `.tooned/` to
    // `.gitignore` before opening the index means the very first scan
    // never walks into (and tries to index) its own database file.
    gitignore::ensure_ignored(project_root)?;
    let mut conn = schema::open_index(project_root)?;

    let mut summary = ScanSummary::default();

    // A single transaction for the whole scan rather than one implicit
    // transaction per row -- with SQLite's default synchronous durability,
    // per-statement auto-commit means an fsync per file, which turns a
    // thousand-file scan from a sub-second operation into a multi-minute
    // one. This is a straightforward perf requirement (T061b), not a
    // correctness-affecting choice either way (a scan that's interrupted
    // partway through leaves the previous index state intact rather than a
    // half-updated one, which is arguably the more correct failure mode
    // too).
    let tx = conn.transaction()?;

    // A malformed user-supplied exclude pattern is a real configuration
    // error; fail the scan rather than silently ignoring the exclusion list
    // (which would index files the user explicitly asked to skip).
    let exclude_gitignore = filter.compile_excludes(project_root)?;

    let walker = build_walker(project_root, filter.respect_gitignore);

    let mut visited: usize = 0;
    for entry in walker {
        visited += 1;
        enforce_walk_cap(visited)?;

        let Ok(entry) = entry else { continue };
        let Some(file_type) = entry.file_type() else { continue };
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let Ok(rel) = path.strip_prefix(project_root) else { continue };
        let Some(rel_str) = rel.to_str() else { continue };
        if is_tooned_internal(rel_str) {
            continue;
        }

        // Check if file is excluded - if so, skip processing
        if !filter.excludes.is_empty() && filter.is_excluded(path, project_root, &exclude_gitignore)
        {
            continue;
        }

        let Ok(meta) = std::fs::metadata(path) else { continue };

        // Check the file's size *before* ever reading its content: a file
        // larger than `max_input_bytes` can never be converted (`inspect`
        // would immediately gate it as `InputTooLarge`), so it must never be
        // fully materialized into memory via `std::fs::read` in the first
        // place -- otherwise the documented size cap is meaningless for
        // scanning (a single huge file anywhere in the tree can OOM the
        // scanner). Oversized files are still fingerprinted (via a streaming
        // hash that never buffers the whole file) and get a `files` row, just
        // no `shapes`/`conversions` rows, exactly matching what `inspect`
        // would have reported anyway.
        let max_input_bytes = ConversionOptions::default().max_input_bytes;
        let classified = if meta.len() > max_input_bytes as u64 {
            // For oversized files, read a prefix to detect type
            let doc_type = detect_file_type(path, filter);
            if !filter.matches_type(doc_type) {
                continue;
            }
            let Ok(content_hash) = hash_file_streaming(path) else { continue };
            persist_oversized_file(
                &tx,
                rel_str,
                &content_hash,
                meta.len(),
                file_mtime_unix(&meta),
            )?;
            false
        } else {
            let Ok(bytes) = std::fs::read(path) else { continue };
            let doc_type = tooned_core::inspect(&bytes, &ConversionOptions::default()).doc_type;
            if !filter.matches_type(doc_type) {
                continue;
            }
            persist_scanned_file(&tx, rel_str, &bytes, meta.len(), file_mtime_unix(&meta))?
        };
        summary.files_scanned += 1;
        if classified {
            summary.files_classified += 1;
        }

        if summary.files_scanned % 100 == 0 {
            eprint!(
                "\rscanned {} files ({} classified)...",
                summary.files_scanned, summary.files_classified
            );
        }
    }

    if summary.files_scanned >= 100 {
        eprintln!();
    }

    tx.commit()?;

    Ok(summary)
}

/// Builds a directory walker over `root` that respects `.gitignore` (and
/// standard VCS-ignore conventions generally) via the `ignore` crate --
/// same mechanism `ripgrep` uses. Hidden entries (dotfiles/dot-directories,
/// including `.git/` and `.tooned/`) are skipped by its default settings.
/// `require_git(false)`: honor `.gitignore` even when `root` isn't itself
/// inside an actual git repository (the `ignore` crate's default is to
/// only apply `.gitignore` rules within a detected git repo) -- a
/// scanned project need not have run `git init` yet for its `.gitignore`
/// to still express the developer's intended ignore rules.
pub(crate) fn build_walker(root: &Path, respect_gitignore: bool) -> ignore::Walk {
    ignore::WalkBuilder::new(root)
        .require_git(false)
        .hidden(true)
        .ignore(respect_gitignore)
        .git_ignore(respect_gitignore)
        .git_global(respect_gitignore)
        .git_exclude(respect_gitignore)
        .build()
}

/// Defense-in-depth check (on top of `ignore`'s default hidden-entry
/// skipping and the `.gitignore` entry `scan_full` itself maintains): never
/// treat `.tooned/`'s own contents as a file to index.
pub(crate) fn is_tooned_internal(rel_path: &str) -> bool {
    rel_path == ".tooned" || rel_path.starts_with(".tooned/")
}

/// Detects the document type of a file by reading a prefix (up to 4 KiB).
/// Used for oversized files where we don't want to read the entire file.
pub(crate) fn detect_file_type(path: &Path, _filter: &IndexFilter) -> Option<DocType> {
    let mut file = File::open(path).ok()?;
    let mut buffer = [0u8; 4096];
    let n = std::io::Read::read(&mut file, &mut buffer).ok()?;
    if n == 0 {
        return None;
    }
    let prefix = buffer.get(..n)?;
    tooned_detect::detect(prefix, None)
}

/// Fingerprints, classifies, and upserts one file's row into
/// `files`/`shapes`/`conversions`. Returns whether the file was a
/// recognized doctype (i.e. got shape/conversion rows too). Stale
/// shape/conversion rows from a prior scan are cleared first, since a
/// file's doctype (and therefore whether it has any) can change between
/// scans.
pub(crate) fn persist_scanned_file(
    conn: &Connection,
    rel_path: &str,
    bytes: &[u8],
    size_bytes: u64,
    mtime_unix: i64,
) -> Result<bool, IndexError> {
    let content_hash = blake3::hash(bytes).to_hex().to_string();
    let report = tooned_core::inspect(bytes, &ConversionOptions::default());
    let scanned_at = now_unix();
    let doc_type_str = report.doc_type.map(doc_type_to_str);

    conn.execute(
        "INSERT INTO files (path, size_bytes, mtime_unix, content_hash, doc_type, scanned_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(path) DO UPDATE SET
            size_bytes = excluded.size_bytes,
            mtime_unix = excluded.mtime_unix,
            content_hash = excluded.content_hash,
            doc_type = excluded.doc_type,
            scanned_at = excluded.scanned_at",
        rusqlite::params![
            rel_path,
            saturating_i64(size_bytes),
            mtime_unix,
            content_hash,
            doc_type_str,
            scanned_at,
        ],
    )?;

    conn.execute("DELETE FROM shapes WHERE path = ?1", [rel_path])?;
    conn.execute("DELETE FROM conversions WHERE path = ?1", [rel_path])?;

    persist_shape_and_conversion(conn, rel_path, &report, scanned_at)?;

    Ok(report.doc_type.is_some())
}

/// Fingerprints a file's content without ever loading it fully into memory,
/// by streaming it through blake3's `Hasher::update_reader` (internally
/// buffered in small fixed-size chunks) rather than `std::fs::read` +
/// `blake3::hash`.
pub(crate) fn hash_file_streaming(path: &Path) -> std::io::Result<String> {
    let file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(file)?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// Persists a `files` row for a file that exceeds `max_input_bytes` and was
/// therefore never read into memory or classified -- mirrors exactly what
/// `persist_scanned_file` would have recorded had `inspect` been given the
/// full content and gated it as `PassthroughReason::InputTooLarge` (no
/// `doc_type`, no `shapes`/`conversions` rows). Kept as a distinct entrypoint
/// so the oversized-file path never needs the file's bytes at all.
pub(crate) fn persist_oversized_file(
    conn: &Connection,
    rel_path: &str,
    content_hash: &str,
    size_bytes: u64,
    mtime_unix: i64,
) -> Result<(), IndexError> {
    let scanned_at = now_unix();
    let doc_type_str: Option<&str> = None;

    conn.execute(
        "INSERT INTO files (path, size_bytes, mtime_unix, content_hash, doc_type, scanned_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(path) DO UPDATE SET
            size_bytes = excluded.size_bytes,
            mtime_unix = excluded.mtime_unix,
            content_hash = excluded.content_hash,
            doc_type = excluded.doc_type,
            scanned_at = excluded.scanned_at",
        rusqlite::params![
            rel_path,
            saturating_i64(size_bytes),
            mtime_unix,
            content_hash,
            doc_type_str,
            scanned_at,
        ],
    )?;

    conn.execute("DELETE FROM shapes WHERE path = ?1", [rel_path])?;
    conn.execute("DELETE FROM conversions WHERE path = ?1", [rel_path])?;

    Ok(())
}

fn persist_shape_and_conversion(
    conn: &Connection,
    rel_path: &str,
    report: &InspectReport,
    computed_at: i64,
) -> Result<(), IndexError> {
    if report.doc_type.is_none() {
        return Ok(());
    }

    let (uniformity_pct, sampled_count) = match &report.shape {
        ShapeClass::UniformArrayOfObjects { uniformity_pct, sampled } => {
            let sampled_i64 = saturating_i64(*sampled as u64);
            (Some(*uniformity_pct), Some(sampled_i64))
        }
        _ => (None, None),
    };
    conn.execute(
        "INSERT INTO shapes (path, json_pointer, shape_class, uniformity_pct, sampled_count)
         VALUES (?1, '', ?2, ?3, ?4)
         ON CONFLICT(path, json_pointer) DO UPDATE SET
            shape_class = excluded.shape_class,
            uniformity_pct = excluded.uniformity_pct,
            sampled_count = excluded.sampled_count",
        rusqlite::params![rel_path, shape_class_str(&report.shape), uniformity_pct, sampled_count],
    )?;

    if let (Some(json_bytes), Some(toon_bytes), Some(savings_pct)) =
        (report.json_bytes, report.toon_bytes, report.savings_pct)
    {
        conn.execute(
            "INSERT INTO conversions (path, json_pointer, json_bytes, toon_bytes, savings_pct, cached_toon_text, computed_at)
             VALUES (?1, '', ?2, ?3, ?4, NULL, ?5)
             ON CONFLICT(path, json_pointer) DO UPDATE SET
                json_bytes = excluded.json_bytes,
                toon_bytes = excluded.toon_bytes,
                savings_pct = excluded.savings_pct,
                computed_at = excluded.computed_at",
            rusqlite::params![
                rel_path,
                saturating_i64(json_bytes as u64),
                saturating_i64(toon_bytes as u64),
                savings_pct,
                computed_at,
            ],
        )?;
    }

    Ok(())
}

fn shape_class_str(shape: &ShapeClass) -> &'static str {
    match shape {
        ShapeClass::UniformArrayOfObjects { .. } => "uniform",
        ShapeClass::Irregular => "irregular",
        ShapeClass::Scalar => "scalar",
        _ => "unknown",
    }
}

fn doc_type_to_str(doc_type: DocType) -> &'static str {
    match doc_type {
        DocType::Json => "json",
        DocType::NdJson => "ndjson",
        DocType::Yaml => "yaml",
        DocType::Toml => "toml",
        DocType::Csv => "csv",
        DocType::Tsv => "tsv",
        DocType::Xml => "xml",
        DocType::Msgpack => "msgpack",
        DocType::Cbor => "cbor",
        DocType::Json5 => "json5",
        _ => "unknown",
    }
}
