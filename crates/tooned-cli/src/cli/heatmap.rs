// SPDX-License-Identifier: AGPL-3.0-only

//! tooned heatmap -- a GitHub/Codex-style contribution calendar of TOON
//! token savings, rendered to the terminal.
//!
//! Honors --global (user-level ledger) vs. the project ledger, and --all
//! to span the full history instead of the last year. --tui is an
//! interactive pager (Enter/n next, p prev, q quit). Nothing here touches the
//! network or reads anything beyond the local metrics ledger.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use tooned_metrics::{
    Metric, QueryOpts, Store, day_to_ymd, today_day, user_global_db_path, ymd_to_day,
};

const PAGE_DAYS: i64 = 364;
const BLOCK: char = '\u{2588}';

fn month_name(m: u32) -> String {
    match m {
        0 => "Jan".into(),
        1 => "Feb".into(),
        2 => "Mar".into(),
        3 => "Apr".into(),
        4 => "May".into(),
        5 => "Jun".into(),
        6 => "Jul".into(),
        7 => "Aug".into(),
        8 => "Sep".into(),
        9 => "Oct".into(),
        10 => "Nov".into(),
        11 => "Dec".into(),
        _ => "?".into(),
    }
}

fn weekday_name(i: usize) -> String {
    match i {
        0 => "Mon".into(),
        1 => "Tue".into(),
        2 => "Wed".into(),
        3 => "Thu".into(),
        4 => "Fri".into(),
        5 => "Sat".into(),
        6 => "Sun".into(),
        _ => "?".into(),
    }
}

// CLI arg struct with clap-generated bool flags; not a state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(clap::Args)]
#[command(
    after_help = "Examples:\n  tooned heatmap\n  tooned heatmap --global\n  tooned heatmap --all --metric bytes\n  tooned heatmap --tui"
)]
pub struct HeatmapArgs {
    /// Read from the user-global ledger instead of the project ledger.
    #[arg(short = 'g', long)]
    pub global: bool,

    /// Show the full history instead of just the last year.
    #[arg(short = 'a', long)]
    pub all: bool,

    /// Launch the interactive TUI pager.
    #[arg(short = 't', long)]
    pub tui: bool,

    /// Metric to display: tokens saved (default) or bytes saved.
    #[arg(short = 'm', long, value_enum, default_value_t = MetricArg::Tokens)]
    pub metric: MetricArg,

    /// Filter to a single surface (e.g. cli:convert, index:scan, hook:claude, mcp:server).
    #[arg(short = 'S', long)]
    pub surface: Option<String>,

    /// Include index-discovered opportunity events in the totals.
    #[arg(short = 'o', long)]
    pub include_opportunity: bool,

    /// Override the start of the range (YYYY-MM-DD).
    #[arg(short = 's', long)]
    pub since: Option<String>,

    /// Override the end of the range (YYYY-MM-DD, default today).
    #[arg(short = 'u', long)]
    pub until: Option<String>,
}

#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MetricArg {
    Tokens,
    Bytes,
}

impl MetricArg {
    fn to_metric(self) -> Metric {
        match self {
            MetricArg::Tokens => Metric::Tokens,
            MetricArg::Bytes => Metric::Bytes,
        }
    }
}

fn parse_ymd(s: &str) -> anyhow::Result<String> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        anyhow::bail!("invalid date {:?}, expected YYYY-MM-DD", s);
    }
    let y = parts
        .first()
        .ok_or_else(|| anyhow::anyhow!("bad year"))?
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("bad year"))?;
    let m = parts
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("bad month"))?
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("bad month"))?;
    let d = parts
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("bad day"))?
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("bad day"))?;
    Ok(format!("{:04}-{:02}-{:02}", y, m, d))
}

fn opts_from(args: &HeatmapArgs) -> anyhow::Result<QueryOpts<'_>> {
    let mut opts = QueryOpts {
        since_day: None,
        until_day: None,
        by: args.metric.to_metric(),
        include_opportunity: args.include_opportunity,
        surface: args.surface.as_deref(),
    };
    if let Some(s) = &args.since {
        let ds = parse_ymd(s)?;
        opts.since_day = Some(ymd_to_day(&ds).ok_or_else(|| anyhow::anyhow!("bad date"))?);
    }
    if let Some(u) = &args.until {
        let ds = parse_ymd(u)?;
        opts.until_day = Some(ymd_to_day(&ds).ok_or_else(|| anyhow::anyhow!("bad date"))?);
    }
    if args.all {
        // Span the full recorded history.
        opts.since_day = Some(0);
        opts.until_day = Some(today_day());
    } else if opts.since_day.is_none() {
        opts.since_day = Some(today_day() - 364i64);
    }
    Ok(opts)
}

fn ledger_path(global: bool) -> PathBuf {
    if global {
        user_global_db_path()
    } else {
        let root = crate::metrics_recorder::current_project_root()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        tooned_metrics::project_db_path(&root)
    }
}

fn since_of(opts: &QueryOpts) -> i64 {
    let default = 0i64;
    match opts.since_day {
        Some(v) => v,
        None => default,
    }
}

fn until_of(opts: &QueryOpts) -> i64 {
    let default = 0i64;
    match opts.until_day {
        Some(v) => v,
        None => default,
    }
}

fn run_cli(ledger: &Store, args: &HeatmapArgs, opts: &QueryOpts) -> anyhow::Result<()> {
    let metric = args.metric.to_metric();
    let cells = ledger.heatmap(opts)?;
    let summary = ledger.summary(opts)?;
    print_summary_line(&summary, metric, args.global);
    println!();
    render_month_labels(&cells);
    let grid = render_grid(&cells);
    for (i, row) in grid.iter().enumerate() {
        println!("  {:<3}{}", weekday_name(i), row);
    }
    print_legend();
    print_month_strip(&cells);
    Ok(())
}

fn run_tui(ledger: &Store, args: &HeatmapArgs, opts: &QueryOpts) -> anyhow::Result<()> {
    let metric = args.metric.to_metric();
    let mut offset: i64 = 0;
    let stdin = io::stdin();
    let all = args.all;
    loop {
        let page_opts = if all {
            opts.clone()
        } else {
            let end = match opts.until_day {
                Some(v) => v,
                None => today_day(),
            };
            let until = end.saturating_sub(offset);
            let since = until.saturating_sub(PAGE_DAYS);
            let since_day_val = match opts.since_day {
                Some(v) => v,
                None => since,
            };
            QueryOpts {
                since_day: Some(since.max(since_day_val)),
                until_day: Some(until),
                by: opts.by,
                include_opportunity: opts.include_opportunity,
                surface: opts.surface,
            }
        };
        print!("\x1b[2J\x1b[H");
        io::stdout().flush()?;
        let cells = ledger.heatmap(&page_opts)?;
        let summary = ledger.summary(&page_opts)?;
        print_summary_line(&summary, metric, args.global);
        println!();
        render_month_labels(&cells);
        let grid = render_grid(&cells);
        for (i, row) in grid.iter().enumerate() {
            println!("  {:<3}{}", weekday_name(i), row);
        }
        print_legend();
        print_month_strip(&cells);
        println!();
        if all {
            println!("full history | press q to quit");
        } else {
            println!(
                "range {}-{} | Enter/n next  p prev  q quit",
                day_to_ymd(since_of(&page_opts)),
                day_to_ymd(until_of(&page_opts))
            );
        }
        let mut buf = String::new();
        if stdin.lock().read_line(&mut buf).is_err() {
            break;
        }
        let cmd = buf.trim();
        if all {
            match cmd {
                "q" | "Q" => break,
                _ => {}
            }
        } else {
            match cmd {
                "" | "n" | "f" => offset = offset.saturating_add(PAGE_DAYS),
                "p" | "b" => offset = offset.saturating_sub(PAGE_DAYS),
                "q" | "Q" => break,
                _ => {}
            }
        }
    }
    Ok(())
}

pub fn run(args: &HeatmapArgs) -> anyhow::Result<()> {
    let path = ledger_path(args.global);
    let Ok(ledger) = Store::open(&path) else {
        println!("no metrics recorded yet");
        return Ok(());
    };
    let opts = opts_from(args)?;
    if args.tui { run_tui(&ledger, args, &opts) } else { run_cli(&ledger, args, &opts) }
}

fn render_grid(cells: &[tooned_metrics::HeatmapCell]) -> [String; 7] {
    let mut rows: [String; 7] = Default::default();
    for c in cells {
        // The result is always in range 0-6, safe to cast
        #[allow(clippy::cast_sign_loss)]
        let i = (((c.day % 7) + 3) % 7) as usize;
        if let Some(row) = rows.get_mut(i) {
            row.push_str(&color_for_level(c.level));
            row.push(BLOCK);
            row.push_str("\x1b[0m");
        }
    }
    rows
}

fn color_for_level(level: u8) -> String {
    match level {
        0 => "\x1b[38;5;237m".to_string(),
        1 => "\x1b[38;2;40;90;40m".to_string(),
        2 => "\x1b[38;2;60;150;60m".to_string(),
        3 => "\x1b[38;2;80;200;80m".to_string(),
        _ => "\x1b[38;2;110;240;110m".to_string(),
    }
}

fn render_month_labels(cells: &[tooned_metrics::HeatmapCell]) {
    print!("    ");
    let mut last = 255u32;
    for (col, c) in cells.iter().enumerate() {
        // The result is always in range 0-11, safe to cast
        #[allow(clippy::cast_sign_loss)]
        let m = ((c.day / 30) % 12) as u32;
        if m != last && col.is_multiple_of(3) {
            print!("{:<3}", month_name(m));
            last = m;
        } else {
            print!("   ");
        }
    }
    println!();
}

fn print_month_strip(cells: &[tooned_metrics::HeatmapCell]) {
    if cells.is_empty() {
        return;
    }
    let first = cells.first().map_or(0i64, |c| c.day);
    let last = cells.last().map_or(0i64, |c| c.day);
    println!("    {} .. {}", day_to_ymd(first), day_to_ymd(last));
}

fn level_color(level: u8) -> String {
    match level {
        0 => "\x1b[38;5;237m".to_string(),
        1 => "\x1b[38;2;40;90;40m".to_string(),
        2 => "\x1b[38;2;60;150;60m".to_string(),
        3 => "\x1b[38;2;80;200;80m".to_string(),
        _ => "\x1b[38;2;110;240;110m".to_string(),
    }
}

fn format_count(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn format_rate(pct: f64) -> String {
    if pct >= 100.0 {
        "100%".to_string()
    } else if pct <= 0.0 {
        "0%".to_string()
    } else {
        format!("{:.0}%", pct)
    }
}

fn print_summary_line(s: &tooned_metrics::Summary, metric: Metric, global: bool) {
    #[allow(clippy::manual_unwrap_or)]
    let (saved, label) = match metric {
        Metric::Tokens => (
            match s.total_tokens_saved.try_into() {
                Ok(v) => v,
                Err(_) => i64::MAX,
            },
            "tokens saved",
        ),
        Metric::Bytes => (
            match s.total_saved_bytes.try_into() {
                Ok(v) => v,
                Err(_) => i64::MAX,
            },
            "bytes saved",
        ),
    };
    let scope = if global { "user" } else { "project" };
    println!(
        "{} {} | {} conversions, {} passthroughs, {} events",
        format_count(saved),
        label,
        s.conversions,
        s.passthroughs,
        s.total_events
    );
    #[allow(clippy::manual_unwrap_or)]
    let busiest = match s.busiest_value.try_into() {
        Ok(v) => v,
        Err(_) => i64::MAX,
    };
    println!(
        "{} | avg reduction {} | busiest {} ({} saved) | streak {}d | span {}d",
        scope,
        format_rate(s.avg_reduction_pct),
        s.busiest_day,
        format_count(busiest),
        s.current_streak_days,
        s.span_days
    );
}

fn print_legend() {
    let mut s = String::from("    less ");
    for lv in 0u8..=4 {
        s.push_str(&level_color(lv));
        s.push(BLOCK);
    }
    s.push_str("\x1b[0m more");
    println!("{}", s);
}
