// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned metrics` -- inspect the local token-savings ledger.
//!
//! Subcommands: `summary` (default), `breakdown` (per-surface),
//! `top` (leaderboard by file or project), `recent`, `export` (JSON/CSV),
//! and `reset` (with explicit confirmation). All reads are best-effort: a
//! missing/empty ledger is reported, never a panic (constitution Principle I).

#![allow(clippy::cast_sign_loss)]

use std::path::PathBuf;

use anyhow::bail;
use clap::{Args, Subcommand};

use tooned_metrics::{
    EventRow, ExportFormat, HeatmapCell, Metric, PerSurface, QueryOpts, Store, Summary, TopFile,
    day_to_ymd, user_global_db_path, ymd_to_day,
};

/// `tooned metrics <subcommand>`
#[derive(Debug, Args)]
pub struct MetricsArgs {
    /// Read the user-global ledger instead of the project-scoped one
    /// (`<root>/.tooned/metrics.db`). Auto-detects the project root (cwd, or
    /// the nearest ancestor with a `.tooned/` directory).
    #[arg(long)]
    pub global: bool,

    #[command(subcommand)]
    pub command: MetricsCommand,
}

#[derive(Debug, Subcommand)]
pub enum MetricsCommand {
    /// Roll-up summary of saved tokens/bytes over the window (default).
    Summary(MetricsWindow),
    /// Per-surface breakdown (one row per originating surface).
    Breakdown(MetricsWindow),
    /// Leaderboard of the most-saved files or projects.
    Top(TopArgs),
    /// Most recent recorded events, newest first.
    Recent(MetricsWindow),
    /// Export every recorded event as JSON or CSV.
    Export(ExportArgs),
    /// Delete all recorded events from the ledger (requires `--yes`).
    Reset(ResetArgs),
}

#[derive(Clone, Debug, Args)]
pub struct MetricsWindow {
    /// Inclusive lower bound, `YYYY-MM-DD` (default: 365 days before `--until`).
    #[arg(long)]
    pub since: Option<String>,
    /// Inclusive upper bound, `YYYY-MM-DD` (default: today).
    #[arg(long)]
    pub until: Option<String>,
    /// Metric to aggregate: `tokens` (default) or `bytes`.
    #[arg(long, value_enum)]
    pub metric: Option<MetricArg>,
    /// Also count `index` opportunity events (not just actual conversions).
    #[arg(long)]
    pub opportunity: bool,
    /// Restrict to a single surface string (e.g. `hook:claude`).
    #[arg(long)]
    pub surface: Option<String>,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum MetricArg {
    Tokens,
    Bytes,
}

impl MetricArg {
    pub(crate) fn to_metric(self) -> Metric {
        match self {
            MetricArg::Tokens => Metric::Tokens,
            MetricArg::Bytes => Metric::Bytes,
        }
    }
}

#[derive(Debug, Args)]
pub struct TopArgs {
    /// Leaderboard dimension: `file` (default) or `project`.
    #[arg(long, value_enum)]
    pub by: Option<TopByArg>,
    /// Number of rows to show (default 10).
    #[arg(long)]
    pub n: Option<u32>,
    #[command(flatten)]
    pub window: MetricsWindow,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum TopByArg {
    File,
    Project,
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Output format: `json` (default), `csv`, `prometheus`, or `otel`.
    #[arg(long, value_enum)]
    pub format: Option<ExportFormatArg>,
    /// Write to this file instead of stdout.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Inclusive lower bound, `YYYY-MM-DD`.
    #[arg(long)]
    pub since: Option<String>,
    /// Inclusive upper bound, `YYYY-MM-DD` (default: today).
    #[arg(long)]
    pub until: Option<String>,
    /// Push metrics to this URL (requires --format prometheus or otel).
    /// Enables periodic pushes. Example: http://localhost:9091/metrics
    #[arg(long)]
    pub push_url: Option<String>,
    /// Push interval in seconds (default: 60, requires --push-url).
    #[arg(long)]
    pub push_interval: Option<u64>,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportFormatArg {
    Json,
    Csv,
    Prometheus,
    Otel,
}

impl ExportFormatArg {
    fn to_format(self) -> ExportFormat {
        match self {
            ExportFormatArg::Json => ExportFormat::Json,
            ExportFormatArg::Csv => ExportFormat::Csv,
            ExportFormatArg::Prometheus => ExportFormat::Prometheus,
            ExportFormatArg::Otel => ExportFormat::Otel,
        }
    }
}

#[derive(Debug, Args)]
pub struct ResetArgs {
    /// Required confirmation: without it, `reset` refuses to delete data.
    #[arg(long)]
    pub yes: bool,
}

/// Resolve the ledger path for the chosen scope (global or project).
pub(crate) fn ledger_path(global: bool) -> anyhow::Result<PathBuf> {
    if global {
        Ok(user_global_db_path())
    } else {
        let root = project_root()?;
        Ok(tooned_metrics::project_db_path(&root))
    }
}

/// Nearest ancestor (or cwd) that contains a `.tooned/` directory.
pub(crate) fn project_root() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let mut dir = cwd.as_path();
    loop {
        if dir.join(".tooned").is_dir() {
            return Ok(dir.to_path_buf());
        }
        dir = match dir.parent() {
            Some(p) => p,
            None => return Ok(cwd),
        };
    }
}

/// Build [`QueryOpts`] from a [`MetricsWindow`].
pub(crate) fn opts_from(w: &MetricsWindow) -> QueryOpts<'_> {
    let since_day = w.since.as_deref().and_then(ymd_to_day);
    let until_day = w.until.as_deref().and_then(ymd_to_day);
    QueryOpts {
        since_day,
        until_day,
        by: w.metric.map_or(Metric::Tokens, MetricArg::to_metric),
        include_opportunity: w.opportunity,
        surface: w.surface.as_deref(),
    }
}

pub fn run(args: &MetricsArgs) -> anyhow::Result<()> {
    let path = ledger_path(args.global)?;
    let store = match Store::open(&path) {
        Ok(s) => s,
        Err(e) => bail!("tooned metrics: cannot open ledger {}: {e}", path.display()),
    };

    match &args.command {
        MetricsCommand::Summary(w) => {
            let summary = store.summary(&opts_from(w))?;
            print_summary(&summary, w.metric.map_or(Metric::Tokens, MetricArg::to_metric));
        }
        MetricsCommand::Breakdown(w) => {
            let rows = store.per_surface(&opts_from(w))?;
            print_breakdown(&rows, w.metric.map_or(Metric::Tokens, MetricArg::to_metric));
        }
        MetricsCommand::Top(t) => {
            let opts = opts_from(&t.window);
            let by_val = match t.by {
                Some(v) => v,
                None => TopByArg::File,
            };
            let default_n = 10;
            let n_val = match t.n {
                Some(v) => v,
                None => default_n,
            };
            let rows = match by_val {
                TopByArg::File => store.top_files(&opts, n_val)?,
                TopByArg::Project => store.top_projects(&opts, n_val)?,
            };
            print_top(&rows, by_val);
        }
        MetricsCommand::Recent(w) => {
            let rows = store.recent(50)?;
            print_recent(&rows);
            let _ = w;
        }
        MetricsCommand::Export(e) => {
            let since = e.since.as_deref().and_then(ymd_to_day);
            let until = e.until.as_deref().and_then(ymd_to_day);
            let format_val = match e.format {
                Some(v) => v,
                None => ExportFormatArg::Json,
            };
            let format = format_val.to_format();
            let text = store.export(format, since, until)?;

            if let Some(push_url) = &e.push_url {
                if !matches!(format, ExportFormat::Prometheus | ExportFormat::Otel) {
                    bail!("--push-url requires --format prometheus or --format otel");
                }
                #[allow(clippy::manual_unwrap_or)]
                let interval = match e.push_interval {
                    Some(v) => v,
                    None => 60,
                };
                push_metrics(push_url, &text, interval)?;
            } else {
                match &e.out {
                    Some(p) => {
                        std::fs::write(p, &text)?;
                        println!(
                            "tooned metrics: exported {} events to {}",
                            count_lines(&text),
                            p.display()
                        );
                    }
                    None => {
                        print!("{text}");
                    }
                }
            }
        }
        MetricsCommand::Reset(r) => {
            if !r.yes {
                bail!(
                    "tooned metrics reset: pass --yes to confirm deletion of all recorded metrics"
                );
            }
            let n = store.count()?;
            store.reset()?;
            println!("tooned metrics: reset ledger ({n} event(s) removed) at {}", path.display());
        }
    }
    Ok(())
}

fn count_lines(text: &str) -> usize {
    text.trim_end().split('\n').filter(|l| !l.is_empty()).count()
}

fn push_metrics(url: &str, body: &str, interval: u64) -> anyhow::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Duration;

    let content_type = if url.contains("/v1/metrics") || url.contains("opentelemetry") {
        "application/json"
    } else {
        "text/plain; version=0.0.4; charset=utf-8"
    };

    loop {
        let mut child = Command::new("curl")
            .arg("-sS")
            .arg("-X")
            .arg("POST")
            .arg("-H")
            .arg(format!("Content-Type: {content_type}"))
            .arg("--data-binary")
            .arg("@-")
            .arg(url)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn `curl` for metrics push: {e}"))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(body.as_bytes()).ok();
        }

        let output = child.wait_with_output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("metrics push to {url} failed: {stderr}");
        }

        if interval == 0 {
            break;
        }
        println!("tooned metrics: pushed to {url}; sleeping {interval}s...");
        thread::sleep(Duration::from_secs(interval));
    }
    Ok(())
}

pub(crate) fn metric_word(m: Metric) -> &'static str {
    match m {
        Metric::Tokens => "tokens",
        Metric::Bytes => "bytes",
    }
}

fn print_summary(s: &Summary, m: Metric) {
    let unit = metric_word(m);
    if s.total_events == 0 {
        println!("tooned metrics -- summary");
        println!("  no metrics recorded yet");
        return;
    }
    println!("tooned metrics -- summary");
    println!("  total saved:    {} {unit}", s.total_saved_bytes);
    println!("  total tokens:  {}", s.total_tokens_saved);
    println!("  conversions:    {} event(s)", s.conversions);
    println!("  passthroughs:  {} event(s)", s.passthroughs);
    println!("  avg reduction:  {:.1}%", s.avg_reduction_pct);
    println!("  busiest day:   {} ({} {unit})", s.busiest_day, s.busiest_value);
    println!("  current streak: {} day(s)", s.current_streak_days);
    println!("  window span:   {} day(s)", s.span_days);
}

fn print_breakdown(rows: &[PerSurface], m: Metric) {
    let unit = metric_word(m);
    println!("tooned metrics -- breakdown by surface ({unit})");
    if rows.is_empty() {
        println!("  (no recorded activity in window)");
        return;
    }
    let default_width = 0;
    let width = match rows.iter().map(|r| r.surface.len()).max() {
        Some(v) => v,
        None => default_width,
    }
    .max(8);
    for r in rows {
        println!(
            "  {:<width$}  {:>10} {unit}  {:>6} conv  {:>6} evt",
            r.surface,
            r.saved_bytes,
            r.conversions,
            r.events,
            width = width,
        );
    }
}

fn print_top(rows: &[TopFile], by: TopByArg) {
    let label = match by {
        TopByArg::File => "file",
        TopByArg::Project => "project",
    };
    println!("tooned metrics -- top {label} by saved bytes");
    if rows.is_empty() {
        println!("  (no ranked {label} in window)");
        return;
    }
    for (i, r) in rows.iter().enumerate() {
        println!(
            "  {:>2}. {:<40}  {:>10} bytes  {:>8} tokens  {:>5} evt",
            i + 1,
            r.label,
            r.saved_bytes,
            r.tokens_saved,
            r.events,
        );
    }
}

fn print_recent(rows: &[EventRow]) {
    println!("tooned metrics -- recent events");
    if rows.is_empty() {
        println!("  (no recorded events)");
        return;
    }
    for r in rows {
        let kind = if r.converted { "conv" } else { "pass" };
        println!(
            "  {}  {:>10}  {:>7}  {:<12}  {:<10}  {}",
            day_to_ymd(r.day),
            r.saved_bytes,
            r.tokens_saved,
            kind,
            r.surface,
            r.source_label.as_deref().map_or("-", |v| v),
        );
    }
}

// Silence dead-code warnings for helper types only used in CLI schema.
#[allow(dead_code)]
fn _assert_cell(_: &HeatmapCell) {}
