// SPDX-License-Identifier: AGPL-3.0-only

//! Shared recorder that funnels conversion outcomes from every tooned surface
//! into the local metrics ledger(s) provided by the `tooned-metrics` crate.
//!
//! Recording is strictly best-effort: any failure (missing DB, locked file,
//! I/O error) is swallowed so a metrics hiccup can never change the behavior or
//! exit code of the command it instruments (the constitution's fail-safe
//! Principle I applies to instrumentation too).
//!
//! Location policy: every event is recorded to the **user-global** ledger and
//! also to the **project** ledger when the current directory sits inside a
//! project that already has a `.tooned/` index. Records are never sent
//! off-machine.

use std::path::{Path, PathBuf};

use tooned_core::DocType;
use tooned_metrics::{Event, EventKind, RecordBuilder, record_event_all};

/// Every instrumented tooned surface. Each variant maps to the `surface`
/// string persisted in the ledger so the views can group/filter by origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliSurface {
    Convert,
    Onto,
    Tron,
    Decode,
    Diff,
    Pipe,
    Wrap,
    Index,
    Check,
    TokenSavings,
    HookClaude,
    HookCodex,
    HookDevin,
    HookDroid,
    HookOpenCode,
    HookKilo,
    HookPi,
    McpServer,
}

impl CliSurface {
    /// The stable `surface` string stored for this scope.
    pub fn surface(self) -> &'static str {
        match self {
            CliSurface::Convert => "cli:convert",
            CliSurface::Onto => "cli:onto",
            CliSurface::Tron => "cli:tron",
            CliSurface::Decode => "cli:decode",
            CliSurface::Diff => "cli:diff",
            CliSurface::Pipe => "cli:pipe",
            CliSurface::Wrap => "cli:wrap",
            CliSurface::Index => "index:scan",
            CliSurface::Check => "cli:check",
            CliSurface::TokenSavings => "cli:token-savings",
            CliSurface::HookClaude => "hook:claude",
            CliSurface::HookCodex => "hook:codex",
            CliSurface::HookDevin => "hook:devin",
            CliSurface::HookDroid => "hook:droid",
            CliSurface::HookOpenCode => "hook:opencode",
            CliSurface::HookKilo => "hook:kilo",
            CliSurface::HookPi => "hook:pi",
            CliSurface::McpServer => "mcp:server",
        }
    }
}

/// A (deliberately opaque) source label for a recorded event: a path relative
/// to the project root, or nothing for stdin/streams. Never an absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLabel {
    None,
    Label(String),
}

impl SourceLabel {
    /// Build a label from a CLI input path. Stdin (`-`) and unknown paths
    /// record as `None`; files are stored relative to the current directory
    /// when possible, falling back to the bare file name.
    pub fn from_path(path: &Path) -> SourceLabel {
        if path == Path::new("-") {
            return SourceLabel::None;
        }
        let cwd = std::env::current_dir().ok();
        let rel = cwd
            .as_ref()
            .and_then(|c| path.strip_prefix(c).ok())
            .map(|p| p.to_string_lossy().into_owned());
        let s = rel
            .or_else(|| path.file_name().map(|f| f.to_string_lossy().into_owned()))
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        SourceLabel::Label(s)
    }

    fn as_opt(&self) -> Option<String> {
        match self {
            SourceLabel::None => None,
            SourceLabel::Label(s) => Some(s.clone()),
        }
    }
}

/// Options shared by the conversion-path CLI subcommands; used to derive a
/// metrics event from a completed `maybe_tooned` / forced-conversion result.
pub struct ConvertOutcome {
    pub scope: CliSurface,
    pub source_label: SourceLabel,
    pub doc_type: String,
    pub converted: bool,
    pub input_bytes: i64,
    pub output_bytes: i64,
    /// Optional model-aware token-savings figure (measured by the caller via
    /// `tooned-token`). When `Some`, it overrides the heuristic default and
    /// flips `precise` on in the stored event.
    pub tokens_saved: Option<u64>,
    #[allow(dead_code)]
    pub precise: bool,
}

impl ConvertOutcome {
    /// Compute the byte-savings percentage and record the event to both the
    /// global and project ledgers.
    pub fn record(self) {
        let mut builder = RecordBuilder::new(self.scope.surface())
            .kind(EventKind::Actual)
            .source_label(self.source_label.as_opt())
            .doc_type(if self.doc_type.is_empty() { None } else { Some(self.doc_type) })
            .converted(self.converted)
            .sizes(
                #[allow(clippy::manual_unwrap_or)]
                match self.input_bytes.max(0).try_into() {
                    Ok(v) => v,
                    Err(_) => u64::MAX,
                },
                #[allow(clippy::manual_unwrap_or)]
                match self.output_bytes.max(0).try_into() {
                    Ok(v) => v,
                    Err(_) => u64::MAX,
                },
            );
        if let Some(tokens) = self.tokens_saved {
            // The ledger records `precise` as a boolean only; the concrete
            // tokenizer profile is not persisted (kept out of the schema to
            // avoid a migration).
            builder = builder.tokens_saved(tokens);
        }
        record(&builder.build());
    }
}

/// Record an event to the global + (when present) project ledgers. Never
/// returns an error to the caller. Project root is detected by walking up from
/// cwd for an existing `.tooned/` directory; `record_event_all` already
/// no-ops when metrics are disabled and swallows every error internally.
fn record(event: &Event) {
    let root = current_project_root();
    record_event_all(event, root.as_deref());
}

/// Detect a project root: the nearest ancestor (or cwd itself) that contains a
/// `.tooned/` directory.
pub fn current_project_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let root = tooned_core::project_root(&cwd);
    if root.join(".tooned").is_dir() { Some(root) } else { None }
}

/// Build a [`SourceLabel`] from a CLI input path (or `None` for stdin).
pub fn label_from_path(path: &Path) -> SourceLabel {
    SourceLabel::from_path(path)
}

/// Record a one-call conversion outcome used by the conversion-path
/// subcommands once the outcome (and byte counts) are known.
pub fn record_convert_outcome(
    scope: CliSurface,
    source_label: &SourceLabel,
    doc_type: Option<DocType>,
    converted: bool,
    input_bytes: i64,
    output_bytes: i64,
) {
    let dt = doc_type.map_or_else(String::new, |d| format!("{d:?}"));
    ConvertOutcome {
        scope,
        source_label: source_label.clone(),
        doc_type: dt,
        converted,
        input_bytes,
        output_bytes,
        tokens_saved: None,
        precise: false,
    }
    .record();
}

/// Like [`record_convert_outcome`], but carries an explicit model-aware
/// token-savings figure (F1/F2) so the ledger records precise (not heuristic)
/// savings when the caller measured them via `tooned-token`.
#[allow(clippy::too_many_arguments)]
pub fn record_convert_outcome_ex(
    scope: CliSurface,
    source_label: &SourceLabel,
    doc_type: Option<DocType>,
    converted: bool,
    input_bytes: i64,
    output_bytes: i64,
    tokens_saved: Option<u64>,
    precise: bool,
) {
    let dt = doc_type.map_or_else(String::new, |d| format!("{d:?}"));
    ConvertOutcome {
        scope,
        source_label: source_label.clone(),
        doc_type: dt,
        converted,
        input_bytes,
        output_bytes,
        tokens_saved,
        precise,
    }
    .record();
}

/// Record a non-conversion "activity" event (index scan/sync, hook config).
/// It increments the heatmap's event count but contributes zero to byte/token
/// savings, so it never distorts the savings aggregates.
pub fn record_activity(scope: CliSurface, doc_type: &str) {
    let event = RecordBuilder::new(scope.surface())
        .kind(EventKind::Actual)
        .doc_type(Some(doc_type.to_string()))
        .converted(false)
        .sizes(0, 0)
        .build();
    record(&event);
}
