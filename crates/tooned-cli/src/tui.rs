// SPDX-License-Identifier: AGPL-3.0-only

//! Interactive ratatui dashboard for `tooned dashboard`.
//!
//! Pre-loads all metrics data (the ledger is small) and renders a tabbed UI
//! with KPI big-text, trend charts, top-file/agent leaderboards, recent
//! events, and the GitHub-style savings heatmap.

use std::io::{self, IsTerminal as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context as _;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize as _};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, Cell, Chart, Dataset, Gauge, GraphType, List, ListItem, ListState, Paragraph, Row,
    Sparkline, Table, TableState, Tabs,
};
use tui_bar_graph::{BarGraph as TuiBarGraph, BarStyle, ColorMode};
use tui_big_text::{BigText, PixelSize};

use crate::cli::metrics::{MetricsWindow, metric_word, opts_from};
use tooned_metrics::{
    EventRow, HeatmapCell, Metric, PerSurface, QueryOpts, Store, Summary, TopFile,
};

const BLOCK: char = '\u{2588}';
const HEATMAP_WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Summary,
    Trend,
    Top,
    Agents,
    Recent,
    Heatmap,
}

impl Tab {
    fn as_str(self) -> &'static str {
        match self {
            Tab::Summary => "Summary",
            Tab::Trend => "Trend",
            Tab::Top => "Top",
            Tab::Agents => "Agents",
            Tab::Recent => "Recent",
            Tab::Heatmap => "Heatmap",
        }
    }

    fn next(self) -> Self {
        match self {
            Tab::Summary => Tab::Trend,
            Tab::Trend => Tab::Top,
            Tab::Top => Tab::Agents,
            Tab::Agents => Tab::Recent,
            Tab::Recent => Tab::Heatmap,
            Tab::Heatmap => Tab::Summary,
        }
    }

    fn prev(self) -> Self {
        match self {
            Tab::Summary => Tab::Heatmap,
            Tab::Trend => Tab::Summary,
            Tab::Top => Tab::Trend,
            Tab::Agents => Tab::Top,
            Tab::Recent => Tab::Agents,
            Tab::Heatmap => Tab::Recent,
        }
    }
}

struct DashboardData {
    summary: Summary,
    heatmap: Vec<HeatmapCell>,
    per_surface: Vec<PerSurface>,
    top_files: Vec<TopFile>,
    recent: Vec<EventRow>,
    metric: Metric,
}

impl DashboardData {
    fn load(store: &Store, metric: Metric, opts: &QueryOpts<'_>) -> anyhow::Result<Self> {
        let mut query_opts = opts.clone();
        query_opts.by = metric;
        let summary =
            store.summary(&query_opts).context("tooned dashboard: failed to load summary")?;
        let heatmap =
            store.heatmap(&query_opts).context("tooned dashboard: failed to load heatmap")?;
        let per_surface = store
            .per_surface(&query_opts)
            .context("tooned dashboard: failed to load per-surface breakdown")?;
        let top_files = store
            .top_files(&query_opts, 10)
            .context("tooned dashboard: failed to load top files")?;
        let recent = store.recent(100).context("tooned dashboard: failed to load recent events")?;
        Ok(Self { summary, heatmap, per_surface, top_files, recent, metric })
    }
}

struct App {
    path: PathBuf,
    window: MetricsWindow,
    global: bool,
    data: DashboardData,
    tab: Tab,
    table_state: TableState,
    agent_table_state: TableState,
    list_state: ListState,
    running: bool,
}

impl App {
    fn new(path: PathBuf, window: MetricsWindow, global: bool, data: DashboardData) -> Self {
        let mut table_state = TableState::default();
        let mut agent_table_state = TableState::default();
        let mut list_state = ListState::default();
        table_state.select_first();
        agent_table_state.select_first();
        list_state.select_first();
        Self {
            path,
            window,
            global,
            data,
            tab: Tab::Summary,
            table_state,
            agent_table_state,
            list_state,
            running: true,
        }
    }

    fn metric(&self) -> Metric {
        self.data.metric
    }

    fn reload(&mut self) -> anyhow::Result<()> {
        let store = Store::open(&self.path).map_err(|e| {
            anyhow::anyhow!("tooned dashboard: cannot open ledger {}: {e}", self.path.display())
        })?;
        let opts = opts_from(&self.window);
        let metric =
            self.window.metric.map_or(Metric::Tokens, super::cli::metrics::MetricArg::to_metric);
        self.data = DashboardData::load(&store, metric, &opts)?;
        self.table_state.select_first();
        self.agent_table_state.select_first();
        self.list_state.select_first();
        Ok(())
    }

    fn toggle_scope(&mut self) -> anyhow::Result<()> {
        self.global = !self.global;
        self.path = if self.global {
            tooned_metrics::user_global_db_path()
        } else {
            let root = crate::metrics_recorder::current_project_root()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            tooned_metrics::project_db_path(&root)
        };
        self.reload()
    }

    fn toggle_metric(&mut self) -> anyhow::Result<()> {
        let new_metric = match self.metric() {
            Metric::Tokens => Metric::Bytes,
            Metric::Bytes => Metric::Tokens,
        };
        self.window.metric = match new_metric {
            Metric::Tokens => Some(crate::cli::metrics::MetricArg::Tokens),
            Metric::Bytes => Some(crate::cli::metrics::MetricArg::Bytes),
        };
        self.reload()
    }

    fn run_loop(&mut self, terminal: &mut ratatui::DefaultTerminal) -> anyhow::Result<()> {
        let tick = Duration::from_millis(100);
        while self.running {
            terminal.draw(|frame| self.draw(frame))?;
            if !event::poll(tick)? {
                continue;
            }
            if let Event::Key(key) = event::read()? {
                self.handle_key(key)?;
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Char('1') => self.tab = Tab::Summary,
            KeyCode::Char('2') => self.tab = Tab::Trend,
            KeyCode::Char('3') => self.tab = Tab::Top,
            KeyCode::Char('4') => self.tab = Tab::Agents,
            KeyCode::Char('5') => self.tab = Tab::Recent,
            KeyCode::Char('6') => self.tab = Tab::Heatmap,
            KeyCode::Tab => self.tab = self.tab.next(),
            KeyCode::BackTab => self.tab = self.tab.prev(),
            KeyCode::Char('g' | 'G') => self.toggle_scope()?,
            KeyCode::Char('m' | 'M') => self.toggle_metric()?,
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
            KeyCode::Home => self.select_first(),
            KeyCode::End => self.select_last(),
            _ => {}
        }
        Ok(())
    }

    fn select_next(&mut self) {
        match self.tab {
            Tab::Top => self.table_state.select_next(),
            Tab::Agents => self.agent_table_state.select_next(),
            Tab::Recent => self.list_state.select_next(),
            _ => {}
        }
    }

    fn select_prev(&mut self) {
        match self.tab {
            Tab::Top => self.table_state.select_previous(),
            Tab::Agents => self.agent_table_state.select_previous(),
            Tab::Recent => self.list_state.select_previous(),
            _ => {}
        }
    }

    fn select_first(&mut self) {
        match self.tab {
            Tab::Top => self.table_state.select_first(),
            Tab::Agents => self.agent_table_state.select_first(),
            Tab::Recent => self.list_state.select_first(),
            _ => {}
        }
    }

    fn select_last(&mut self) {
        match self.tab {
            Tab::Top => self.table_state.select_last(),
            Tab::Agents => self.agent_table_state.select_last(),
            Tab::Recent => self.list_state.select_last(),
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let [header, main, footer] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
                .areas(area);

        self.render_header(frame, header);
        self.render_main(frame, main);
        Self::render_footer(frame, footer);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let [tabs_area, info_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(30)]).areas(area);

        let titles: Vec<Line<'_>> =
            [Tab::Summary, Tab::Trend, Tab::Top, Tab::Agents, Tab::Recent, Tab::Heatmap]
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let s = t.as_str();
                    if self.tab_index() == i {
                        Line::from(format!("{}:{}", i + 1, s)).green().bold()
                    } else {
                        Line::from(format!("{}:{}", i + 1, s))
                    }
                })
                .collect();

        let tabs = Tabs::new(titles)
            .select(self.tab_index())
            .highlight_style(Style::new().green().bold())
            .divider(" | ");
        frame.render_widget(tabs, tabs_area);

        let scope = if self.global { "user" } else { "project" };
        let unit = metric_word(self.metric());
        let info =
            Paragraph::new(Line::from(vec![scope.to_string().cyan(), " | ".into(), unit.yellow()]))
                .alignment(Alignment::Right);
        frame.render_widget(info, info_area);
    }

    fn tab_index(&self) -> usize {
        match self.tab {
            Tab::Summary => 0,
            Tab::Trend => 1,
            Tab::Top => 2,
            Tab::Agents => 3,
            Tab::Recent => 4,
            Tab::Heatmap => 5,
        }
    }

    fn render_main(&mut self, frame: &mut Frame, area: Rect) {
        match self.tab {
            Tab::Summary => self.render_summary(frame, area),
            Tab::Trend => self.render_trend(frame, area),
            Tab::Top => self.render_top(frame, area),
            Tab::Agents => self.render_agents(frame, area),
            Tab::Recent => self.render_recent(frame, area),
            Tab::Heatmap => self.render_heatmap(frame, area),
        }
    }

    fn render_footer(frame: &mut Frame, area: Rect) {
        let help = Paragraph::new(Line::from(vec![
            "q quit".gray(),
            "  ".into(),
            "1-6 tab".gray(),
            "  ".into(),
            "g scope".gray(),
            "  ".into(),
            "m metric".gray(),
            "  ".into(),
            "j/k move".gray(),
        ]));
        frame.render_widget(help, area);
    }

    fn render_summary(&self, frame: &mut Frame, area: Rect) {
        let [left, right] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(area);
        let [top_big, top_small] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(4)]).areas(left);

        let s = &self.data.summary;
        let unit = metric_word(self.metric());
        let value = match self.metric() {
            Metric::Tokens => s.total_tokens_saved,
            Metric::Bytes => s.total_saved_bytes,
        };

        let big = BigText::builder()
            .pixel_size(PixelSize::HalfHeight)
            .style(Style::new().cyan())
            .lines(vec![Line::from(format_count(value)).cyan().bold(), Line::from(unit).gray()])
            .build();
        frame.render_widget(big, top_big);

        let stats = Paragraph::new(vec![
            Line::from(format!("conversions: {}", s.conversions)),
            Line::from(format!("passthroughs: {}", s.passthroughs)),
            Line::from(format!("events: {}", s.total_events)),
            Line::from(format!("streak: {}d", s.current_streak_days)),
        ])
        .block(Block::bordered().title("Summary"));
        frame.render_widget(stats, top_small);

        let [gauge_area, spark_area] =
            Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).areas(right);
        let pct = (s.avg_reduction_pct / 100.0).clamp(0.0, 1.0);
        let gauge = Gauge::default()
            .block(Block::bordered().title("avg reduction"))
            .ratio(pct)
            .label(format!("{:.1}%", s.avg_reduction_pct))
            .gauge_style(Style::new().green())
            .style(Style::new().white());
        frame.render_widget(gauge, gauge_area);

        let (spark_data, spark_max) = self.sparkline_data(30);
        let spark = Sparkline::default()
            .block(Block::bordered().title("30-day trend"))
            .data(spark_data.iter().copied())
            .max(spark_max)
            .style(Style::new().cyan());
        frame.render_widget(spark, spark_area);
    }

    fn sparkline_data(&self, days: usize) -> (Vec<u64>, u64) {
        let mut data: Vec<u64> =
            self.data.heatmap.iter().rev().take(days).map(|c| c.value).collect();
        data.reverse();
        let max = data.iter().copied().fold(0, u64::max);
        (data, max.max(1))
    }

    fn render_trend(&self, frame: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> =
            self.data.heatmap.iter().enumerate().map(|(i, c)| (i as f64, c.value as f64)).collect();
        let max_u64 = self.data.heatmap.iter().map(|c| c.value).fold(0, u64::max).max(1);
        let max = max_u64 as f64;
        let n = data.len().max(1);
        let dataset = Dataset::default()
            .name("saved")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::new().cyan())
            .data(&data);

        let chart = Chart::new(vec![dataset])
            .block(Block::bordered().title("Daily savings trend"))
            .x_axis(
                Axis::default().title("day").bounds([0.0, (n.saturating_sub(1)) as f64]).labels(
                    vec![
                        Span::raw("0"),
                        Span::raw(format!("{}", n / 2)),
                        Span::raw(format!("{}", n)),
                    ],
                ),
            )
            .y_axis(Axis::default().title(metric_word(self.metric())).bounds([0.0, max]).labels(
                vec![
                    Span::raw("0"),
                    Span::raw(format_count(max_u64 / 2)),
                    Span::raw(format_count(max_u64)),
                ],
            ));
        frame.render_widget(chart, area);
    }

    fn render_top(&mut self, frame: &mut Frame, area: Rect) {
        let [chart_area, table_area] =
            Layout::vertical([Constraint::Length(12), Constraint::Fill(1)]).areas(area);

        let max = self.data.top_files.iter().map(|r| r.saved_bytes).fold(0, u64::max).max(1);
        let values: Vec<f64> = self
            .data
            .top_files
            .iter()
            .map(|r| (r.saved_bytes as f64 / max as f64).clamp(0.0, 1.0))
            .collect();
        let bar_graph = TuiBarGraph::new(values)
            .with_gradient(colorgrad::preset::turbo())
            .with_bar_style(BarStyle::Braille)
            .with_color_mode(ColorMode::VerticalGradient);
        frame.render_widget(bar_graph, chart_area);

        let header =
            Row::new(["#", "file", "saved", "tokens", "events"]).style(Style::new().bold());
        let rows: Vec<Row> = self
            .data
            .top_files
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let label = r.label.clone();
                let truncated = if label.chars().count() > 32 {
                    format!("{}...", label.chars().take(29).collect::<String>())
                } else {
                    label
                };
                Row::new([
                    Cell::from(format!("{}", i + 1)),
                    Cell::from(truncated),
                    Cell::from(format_count(r.saved_bytes)),
                    Cell::from(format_count(r.tokens_saved)),
                    Cell::from(format!("{}", r.events)),
                ])
            })
            .collect();
        let widths = [
            Constraint::Length(4),
            Constraint::Fill(1),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(8),
        ];
        let table = Table::new(rows, widths)
            .header(header)
            .row_highlight_style(Style::new().reversed())
            .block(Block::bordered().title("Top files"));
        frame.render_stateful_widget(table, table_area, &mut self.table_state);
    }

    fn render_agents(&mut self, frame: &mut Frame, area: Rect) {
        let [chart_area, table_area] =
            Layout::vertical([Constraint::Length(12), Constraint::Fill(1)]).areas(area);

        let agents = self.agent_rows();
        let max = agents.iter().map(|a| a.saved).fold(0, u64::max).max(1);
        let values: Vec<f64> =
            agents.iter().map(|a| (a.saved as f64 / max as f64).clamp(0.0, 1.0)).collect();
        let bar_graph = TuiBarGraph::new(values)
            .with_gradient(colorgrad::preset::turbo())
            .with_bar_style(BarStyle::Braille)
            .with_color_mode(ColorMode::VerticalGradient);
        frame.render_widget(bar_graph, chart_area);

        let header = Row::new(["agent", "saved", "tokens", "events", "conv", "share%"])
            .style(Style::new().bold());
        let rows: Vec<Row> = agents
            .iter()
            .map(|a| {
                let total = match self.metric() {
                    Metric::Tokens => self.data.summary.total_tokens_saved,
                    Metric::Bytes => self.data.summary.total_saved_bytes,
                };
                let value = match self.metric() {
                    Metric::Tokens => a.tokens,
                    Metric::Bytes => a.saved,
                };
                let share = if total > 0 { (value as f64 / total as f64) * 100.0 } else { 0.0 };
                Row::new([
                    Cell::from(a.name.clone()),
                    Cell::from(format_count(a.saved)),
                    Cell::from(format_count(a.tokens)),
                    Cell::from(format!("{}", a.events)),
                    Cell::from(format!("{}", a.conversions)),
                    Cell::from(format!("{share:.1}%")),
                ])
            })
            .collect();
        let widths = [
            Constraint::Fill(1),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
        ];
        let table = Table::new(rows, widths)
            .header(header)
            .row_highlight_style(Style::new().reversed())
            .block(Block::bordered().title("Per-agent breakdown"));
        frame.render_stateful_widget(table, table_area, &mut self.agent_table_state);
    }

    fn agent_rows(&self) -> Vec<AgentRow> {
        let mut map: std::collections::BTreeMap<String, AgentRow> =
            std::collections::BTreeMap::new();
        for row in &self.data.per_surface {
            let (name, display) = if let Some(pos) = row.surface.find(':') {
                (row.surface[..pos].to_string(), row.surface[..pos].to_string())
            } else {
                (row.surface.clone(), row.surface.clone())
            };
            let entry = map.entry(name).or_insert_with(|| AgentRow {
                name: display,
                saved: 0,
                tokens: 0,
                events: 0,
                conversions: 0,
            });
            entry.saved += row.saved_bytes;
            entry.tokens += row.tokens_saved;
            entry.events += row.events;
            entry.conversions += row.conversions;
        }
        let mut rows: Vec<AgentRow> = map.into_values().collect();
        rows.sort_by_key(|a| std::cmp::Reverse(a.saved));
        rows
    }

    fn render_recent(&mut self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .data
            .recent
            .iter()
            .map(|r| {
                let kind = if r.converted { "conv" } else { "pass" };
                let line = Line::from(vec![
                    tooned_metrics::day_to_ymd(r.day).cyan(),
                    " ".into(),
                    format_count(r.saved_bytes).green(),
                    " ".into(),
                    kind.gray(),
                    " ".into(),
                    r.surface.as_str().yellow(),
                    " ".into(),
                    r.source_label.as_deref().map_or("-", |v| v).into(),
                ]);
                ListItem::new(line)
            })
            .collect();
        let list = List::new(items)
            .highlight_style(Style::new().reversed())
            .block(Block::bordered().title("Recent events"));
        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_heatmap(&self, frame: &mut Frame, area: Rect) {
        let lines = render_heatmap_grid(&self.data.heatmap);
        let para = Paragraph::new(lines).block(Block::bordered().title("Savings heatmap"));
        frame.render_widget(para, area);
    }
}

struct AgentRow {
    name: String,
    saved: u64,
    tokens: u64,
    events: u64,
    conversions: u64,
}

fn render_heatmap_grid(cells: &[HeatmapCell]) -> Vec<Line<'_>> {
    let mut rows: [Vec<Span>; 7] =
        [Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for c in cells {
        #[allow(clippy::cast_sign_loss)]
        let i = (((c.day % 7) + 3) % 7) as usize;
        if let Some(row) = rows.get_mut(i) {
            row.push(Span::styled(BLOCK.to_string(), heatmap_style(c.level)));
        }
    }

    let mut lines = Vec::with_capacity(9);
    let mut header = vec![Span::raw("    ")];
    let mut last = 255u32;
    for (col, c) in cells.iter().enumerate() {
        #[allow(clippy::cast_sign_loss)]
        let m = ((c.day / 30) % 12) as u32;
        if m != last && col % 3 == 0 {
            header.push(Span::styled(month_name(m), Style::new().cyan()));
            last = m;
        } else {
            header.push(Span::raw("   "));
        }
    }
    lines.push(Line::from(header));

    for (i, row) in rows.iter().enumerate() {
        let label = match HEATMAP_WEEKDAYS.get(i) {
            Some(&label) => label,
            None => "?",
        };
        let mut spans = vec![Span::styled(format!("  {label:<3}"), Style::new().gray())];
        spans.extend(row.iter().cloned());
        lines.push(Line::from(spans));
    }

    if let (Some(first), Some(last)) = (cells.first(), cells.last()) {
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(format!("{} .. {}", first.ymd, last.ymd), Style::new().gray()),
        ]));
    }
    lines
}

fn heatmap_style(level: u8) -> Style {
    let color = match level {
        0 => Color::Rgb(50, 50, 50),
        1 => Color::Rgb(40, 90, 40),
        2 => Color::Rgb(60, 150, 60),
        3 => Color::Rgb(80, 200, 80),
        _ => Color::Rgb(110, 240, 110),
    };
    Style::new().fg(color)
}

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

fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

pub fn run(path: &Path, window: &MetricsWindow, global: bool) -> anyhow::Result<()> {
    if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
        anyhow::bail!("tooned dashboard: a terminal is required; run from an interactive tty");
    }
    let store = Store::open(path).map_err(|e| {
        anyhow::anyhow!("tooned dashboard: cannot open ledger {}: {e}", path.display())
    })?;
    let opts = opts_from(window);
    let metric = window.metric.map_or(Metric::Tokens, super::cli::metrics::MetricArg::to_metric);
    let data = DashboardData::load(&store, metric, &opts)?;

    let mut terminal = ratatui::init();
    let result = {
        let mut app = App::new(path.to_path_buf(), window.clone(), global, data);
        app.run_loop(&mut terminal)
    };
    ratatui::restore();
    result
}
