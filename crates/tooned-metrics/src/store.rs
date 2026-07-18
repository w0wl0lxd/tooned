// SPDX-License-Identifier: AGPL-3.0-only

//! SQLite-backed, local-only metrics ledger.
//!
//! Schema version 1. All writes are best-effort: callers (`record_event*`)
//! swallow every error and never panic.

use std::env;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::SCHEMA_VERSION;
use serde::Serialize;

/// Which classification an event belongs to. `Actual` is a real runtime
/// conversion path (including a passthrough on the hot path); `Opportunity`
/// is a *potential* saving discovered by the index scanner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EventKind {
    Actual,
    Opportunity,
}

impl EventKind {
    fn as_str(self) -> &'static str {
        match self {
            EventKind::Actual => "actual",
            EventKind::Opportunity => "opportunity",
        }
    }
}

/// A single recorded metrics event (in-memory form).
#[derive(Debug, Clone)]
pub struct Event {
    pub surface: String,
    pub at: i64,
    pub kind: EventKind,
    pub project_id: Option<String>,
    pub source_label: Option<String>,
    pub doc_type: Option<String>,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub saved_bytes: u64,
    pub tokens_saved: u64,
    pub converted: bool,
    pub precise: bool,
}

/// Builder for [`Event`]. Computes saved bytes and token estimate from the
/// input/output sizes so callers never have to.
pub struct RecordBuilder {
    surface: String,
    at: Option<i64>,
    kind: EventKind,
    project_id: Option<String>,
    source_label: Option<String>,
    doc_type: Option<String>,
    input_bytes: u64,
    output_bytes: u64,
    converted: bool,
    precise: bool,
    /// Optional explicit token-savings figure (when the caller measured real
    /// model-aware tokens via `tooned-token`). When `Some`, it overrides the
    /// default 4-bytes/token heuristic in [`RecordBuilder::build`].
    tokens_saved_override: Option<u64>,
}

impl RecordBuilder {
    pub fn new(surface: &str) -> Self {
        Self {
            surface: surface.to_string(),
            at: None,
            kind: EventKind::Actual,
            project_id: None,
            source_label: None,
            doc_type: None,
            input_bytes: 0,
            output_bytes: 0,
            converted: false,
            precise: false,
            tokens_saved_override: None,
        }
    }

    #[must_use]
    pub fn at(mut self, ts: i64) -> Self {
        self.at = Some(ts);
        self
    }

    #[must_use]
    pub fn kind(mut self, kind: EventKind) -> Self {
        self.kind = kind;
        self
    }

    #[must_use]
    pub fn project_id(mut self, id: Option<String>) -> Self {
        self.project_id = id;
        self
    }

    #[must_use]
    pub fn source_label(mut self, label: Option<String>) -> Self {
        self.source_label = label;
        self
    }

    #[must_use]
    pub fn doc_type(mut self, dt: Option<String>) -> Self {
        self.doc_type = dt;
        self
    }

    #[must_use]
    pub fn sizes(mut self, input: u64, output: u64) -> Self {
        self.input_bytes = input;
        self.output_bytes = output;
        self
    }

    #[must_use]
    pub fn converted(mut self, yes: bool) -> Self {
        self.converted = yes;
        self
    }

    #[must_use]
    pub fn precise(mut self, yes: bool) -> Self {
        self.precise = yes;
        self
    }

    /// Record an explicit, model-aware token-savings figure (measured by the
    /// caller via `tooned-token`) instead of the heuristic default. `precise`
    /// is forced to `true`.
    #[must_use]
    pub fn tokens_saved(mut self, tokens: u64) -> Self {
        self.tokens_saved_override = Some(tokens);
        self.precise = true;
        self
    }

    pub fn build(self) -> Event {
        let saved = self.input_bytes.saturating_sub(self.output_bytes);
        let tokens = self.tokens_saved_override.unwrap_or_else(|| estimate_tokens(saved));
        Event {
            surface: self.surface,
            at: self.at.unwrap_or_else(now_unix),
            kind: self.kind,
            project_id: self.project_id,
            source_label: self.source_label,
            doc_type: self.doc_type,
            input_bytes: self.input_bytes,
            output_bytes: self.output_bytes,
            saved_bytes: saved,
            tokens_saved: tokens,
            converted: self.converted,
            precise: self.precise,
        }
    }
}

/// Metric aggregated by the heatmap/summary queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Metric {
    #[default]
    Tokens,
    Bytes,
}

/// Export serialization format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Csv,
    Prometheus,
    Otel,
}

/// Filter/aggregation options shared by every query.
#[derive(Debug, Clone, Default)]
pub struct QueryOpts<'a> {
    /// Inclusive lower bound day (days-since-epoch). `None` -> 364 days before `until`.
    pub since_day: Option<i64>,
    /// Inclusive upper bound day. `None` -> today.
    pub until_day: Option<i64>,
    /// Metric to aggregate.
    pub by: Metric,
    /// Include `kind = 'opportunity'` rows (otherwise only actual conversions).
    pub include_opportunity: bool,
    /// Restrict to a single `surface` string.
    pub surface: Option<&'a str>,
}

/// One heatmap cell (one calendar day).
#[derive(Debug, Clone, Serialize)]
pub struct HeatmapCell {
    pub day: i64,
    pub ymd: String,
    pub value: u64,
    pub events: u64,
    pub conversions: u64,
    /// Intensity 0..=4 (0 = no activity / no savings).
    pub level: u8,
}

/// Roll-up summary over the query window.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub total_events: u64,
    pub total_saved_bytes: u64,
    pub total_tokens_saved: u64,
    pub conversions: u64,
    pub passthroughs: u64,
    pub avg_reduction_pct: f64,
    pub busiest_day: String,
    pub busiest_value: u64,
    pub current_streak_days: u64,
    pub span_days: u64,
}

/// Aggregate saved amount for one originating surface.
#[derive(Debug, Clone, Serialize)]
pub struct PerSurface {
    pub surface: String,
    pub saved_bytes: u64,
    pub tokens_saved: u64,
    pub events: u64,
    pub conversions: u64,
}

/// One row of a leaderboard (top files or top projects).
#[derive(Debug, Clone, Serialize)]
pub struct TopFile {
    pub label: String,
    pub saved_bytes: u64,
    pub tokens_saved: u64,
    pub events: u64,
}

/// A single stored event row (used for `recent`/`export`).
#[derive(Debug, Clone, Serialize)]
pub struct EventRow {
    pub ts: i64,
    pub day: i64,
    pub kind: String,
    pub surface: String,
    pub project_id: Option<String>,
    pub source_label: Option<String>,
    pub doc_type: Option<String>,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub saved_bytes: u64,
    pub tokens_saved: u64,
    pub converted: bool,
    pub precise: bool,
}

/// Errors from the metrics store. Never fatal: recording callers swallow them.
#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("refusing to write metrics into a symlinked directory: {0}")]
    Symlink(PathBuf),
    #[error("unsupported metrics schema version: {0} (expected {SCHEMA_VERSION})")]
    UnsupportedSchemaVersion(String),
}

// ---------------------------------------------------------------------------
// Store: writes + queries
// ---------------------------------------------------------------------------

/// An open handle to a metrics database.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (creating if needed) the database at `db_path`. Enforces a
    /// non-symlinked parent and database file, `0600` permissions on the file,
    /// the schema version, and a WAL + busy-timeout for safe concurrent
    /// best-effort writes.
    pub fn open(db_path: &Path) -> Result<Self, MetricsError> {
        ensure_parent(db_path)?;
        refuse_symlink(db_path, "metrics database")?;
        let existed = db_path.exists();
        let conn = Connection::open(db_path).map_err(MetricsError::Sqlite)?;
        if !existed {
            #[cfg(unix)]
            set_mode(db_path, 0o600);
        }
        let _ = conn.execute("PRAGMA journal_mode=WAL", []);
        let _ = conn.execute("PRAGMA busy_timeout=50", []);
        let store = Store { conn };
        store.create_schema_if_needed()?;
        Ok(store)
    }

    fn create_schema_if_needed(&self) -> Result<(), MetricsError> {
        if table_exists(&self.conn, "meta") && table_exists(&self.conn, "events") {
            let version = self.schema_version()?;
            match version {
                Some(ref v) if v.as_str() == SCHEMA_VERSION => return Ok(()),
                Some(v) => return Err(MetricsError::UnsupportedSchemaVersion(v)),
                None => {}
            }
        }
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE IF NOT EXISTS events (
                     ts INTEGER NOT NULL,
                     day INTEGER NOT NULL,
                     kind TEXT NOT NULL,
                     surface TEXT NOT NULL,
                     project_id TEXT,
                     source_label TEXT,
                     doc_type TEXT,
                     input_bytes INTEGER NOT NULL DEFAULT 0,
                     output_bytes INTEGER NOT NULL DEFAULT 0,
                     saved_bytes INTEGER NOT NULL DEFAULT 0,
                     tokens_saved INTEGER NOT NULL DEFAULT 0,
                     converted INTEGER NOT NULL DEFAULT 0,
                     precise INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE INDEX IF NOT EXISTS idx_events_day ON events(day);
                 CREATE INDEX IF NOT EXISTS idx_events_surface ON events(surface);
                 CREATE INDEX IF NOT EXISTS idx_events_kind ON events(kind);",
            )
            .map_err(MetricsError::Sqlite)?;
        // Insert schema version separately - ignore ExecuteReturnedResults error
        let _ = self.conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)",
            [SCHEMA_VERSION],
        );
        Ok(())
    }

    fn schema_version(&self) -> Result<Option<String>, MetricsError> {
        match self.conn.query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| {
            r.get::<_, String>(0)
        }) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(MetricsError::Sqlite(e)),
        }
    }

    /// Record one event. Best-effort from the caller's perspective: returns a
    /// `Result` so it can be swallowed.
    #[allow(clippy::cast_possible_wrap)]
    pub fn record(&self, event: &Event) -> Result<(), MetricsError> {
        self.conn
            .execute(
                "INSERT INTO events \
                 (ts, day, kind, surface, project_id, source_label, doc_type, \
                  input_bytes, output_bytes, saved_bytes, tokens_saved, converted, precise) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    event.at,
                    day_of_ts(event.at),
                    event.kind.as_str(),
                    event.surface,
                    event.project_id,
                    event.source_label,
                    event.doc_type,
                    event.input_bytes as i64,
                    event.output_bytes as i64,
                    event.saved_bytes as i64,
                    event.tokens_saved as i64,
                    i64::from(event.converted),
                    i64::from(event.precise),
                ],
            )
            .map_err(MetricsError::Sqlite)?;
        Ok(())
    }

    /// Number of events in the store.
    pub fn count(&self) -> Result<i64, MetricsError> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .map_err(MetricsError::Sqlite)?;
        Ok(n)
    }

    /// Day-by-day aggregates for the heatmap and summary. Returns a row per
    /// present day with (day, value, events, conversions).
    fn daily_aggregates(
        &self,
        opts: &QueryOpts<'_>,
    ) -> Result<Vec<(i64, u64, u64, u64)>, MetricsError> {
        let col = match opts.by {
            Metric::Tokens => "tokens_saved",
            Metric::Bytes => "saved_bytes",
        };
        let f = filter_clause(opts);
        let sql = format!(
            "SELECT day, COALESCE(SUM({col}),0), COUNT(*), \
             COALESCE(SUM(CASE WHEN converted THEN 1 ELSE 0 END),0) \
             FROM events {where} GROUP BY day ORDER BY day ASC",
            col = col,
            where = f.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql).map_err(MetricsError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(f.binds), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?.max(0) as u64,
                    row.get::<_, i64>(2)?.max(0) as u64,
                    row.get::<_, i64>(3)?.max(0) as u64,
                ))
            })
            .map_err(MetricsError::Sqlite)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(MetricsError::Sqlite)
    }

    /// GitHub/Codex-style per-day heatmap for the query window.
    pub fn heatmap(&self, opts: &QueryOpts<'_>) -> Result<Vec<HeatmapCell>, MetricsError> {
        let (since, until) = window(opts);
        let agg = self.daily_aggregates(opts)?;
        let mut by_day: std::collections::HashMap<i64, (u64, u64, u64)> =
            std::collections::HashMap::new();
        for (day, value, events, conv) in agg {
            by_day.insert(day, (value, events, conv));
        }
        let mut cells: Vec<HeatmapCell> = Vec::new();
        let mut day = since;
        while day <= until {
            let (value, events, conv) =
                by_day.get(&day).map_or((0, 0, 0), |(v, e, c)| (*v, *e, *c));
            cells.push(HeatmapCell {
                day,
                ymd: day_to_ymd(day),
                value,
                events,
                conversions: conv,
                level: 0,
            });
            day += 1;
        }
        assign_levels(&mut cells);
        Ok(cells)
    }

    /// Roll-up summary over the window.
    pub fn summary(&self, opts: &QueryOpts<'_>) -> Result<Summary, MetricsError> {
        let (since, until) = window(opts);
        let f = filter_clause(opts);
        let sql = format!(
            "SELECT COALESCE(SUM(saved_bytes),0), COALESCE(SUM(input_bytes),0), \
             COUNT(*), COALESCE(SUM(CASE WHEN converted THEN 1 ELSE 0 END),0), \
             COALESCE(SUM(tokens_saved),0) \
             FROM events {where}",
            where = f.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql).map_err(MetricsError::Sqlite)?;
        let (saved, input, total, conv, tokens): (i64, i64, i64, i64, i64) = stmt
            .query_row(rusqlite::params_from_iter(f.binds), |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })
            .map_err(MetricsError::Sqlite)?;

        let agg = self.daily_aggregates(opts)?;
        let mut busiest_day = String::new();
        let mut busiest_value: u64 = 0;
        for (day, value, _, _) in &agg {
            if *value > busiest_value {
                busiest_value = *value;
                busiest_day = day_to_ymd(*day);
            }
        }

        // Current streak: consecutive days ending at `until` with any activity.
        let mut streak: u64 = 0;
        let mut cursor = until;
        let present: std::collections::HashSet<i64> = agg.iter().map(|(d, _, _, _)| *d).collect();
        while present.contains(&cursor) {
            streak += 1;
            if cursor == i64::MIN {
                break;
            }
            cursor -= 1;
        }

        let avg = if input > 0 { (saved as f64 / input as f64) * 100.0 } else { 0.0 };

        Ok(Summary {
            total_events: total.max(0) as u64,
            total_saved_bytes: saved.max(0) as u64,
            total_tokens_saved: tokens.max(0) as u64,
            conversions: conv.max(0) as u64,
            passthroughs: (total.max(0) - conv.max(0)).max(0) as u64,
            avg_reduction_pct: avg,
            busiest_day,
            busiest_value,
            current_streak_days: streak,
            span_days: (until - since + 1).max(0) as u64,
        })
    }

    /// Per-surface saved-amount ranking.
    pub fn per_surface(&self, opts: &QueryOpts<'_>) -> Result<Vec<PerSurface>, MetricsError> {
        let col = match opts.by {
            Metric::Tokens => "tokens_saved",
            Metric::Bytes => "saved_bytes",
        };
        let f = filter_clause(opts);
        let sql = format!(
            "SELECT surface, COALESCE(SUM({col}),0), COALESCE(SUM(tokens_saved),0), \
             COUNT(*), COALESCE(SUM(CASE WHEN converted THEN 1 ELSE 0 END),0) \
             FROM events {where} GROUP BY surface ORDER BY SUM({col}) DESC",
            col = col,
            where = f.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql).map_err(MetricsError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(f.binds), |row| {
                Ok(PerSurface {
                    surface: row.get(0)?,
                    saved_bytes: row.get::<_, i64>(1)?.max(0) as u64,
                    tokens_saved: row.get::<_, i64>(2)?.max(0) as u64,
                    events: row.get::<_, i64>(3)?.max(0) as u64,
                    conversions: row.get::<_, i64>(4)?.max(0) as u64,
                })
            })
            .map_err(MetricsError::Sqlite)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(MetricsError::Sqlite)
    }

    /// Top files by saved amount (requires a non-null source label).
    pub fn top_files(
        &self,
        opts: &QueryOpts<'_>,
        top_n: u32,
    ) -> Result<Vec<TopFile>, MetricsError> {
        leaderboard(self, opts, top_n, "source_label")
    }

    /// Top projects by saved amount (requires a non-null project id).
    pub fn top_projects(
        &self,
        opts: &QueryOpts<'_>,
        top_n: u32,
    ) -> Result<Vec<TopFile>, MetricsError> {
        leaderboard(self, opts, top_n, "project_id")
    }

    /// Most recent events, newest first.
    pub fn recent(&self, n: u32) -> Result<Vec<EventRow>, MetricsError> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM events ORDER BY ts DESC LIMIT ?")
            .map_err(MetricsError::Sqlite)?;
        let rows = stmt.query_map([i64::from(n)], event_row_from).map_err(MetricsError::Sqlite)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(MetricsError::Sqlite)
    }

    /// Export raw events as JSON or CSV.
    #[allow(clippy::manual_unwrap_or_default, clippy::manual_unwrap_or)]
    pub fn export(
        &self,
        format: ExportFormat,
        since_day: Option<i64>,
        until_day: Option<i64>,
    ) -> Result<String, MetricsError> {
        let sql = String::from("SELECT * FROM events WHERE day BETWEEN ?1 AND ?2 ORDER BY ts");
        let since = match since_day {
            Some(v) => v,
            None => 0,
        };
        let until = until_day.unwrap_or_else(today_day);
        let mut stmt = self.conn.prepare(&sql).map_err(MetricsError::Sqlite)?;
        let rows = stmt.query_map([since, until], event_row_from).map_err(MetricsError::Sqlite)?;
        let events: Vec<EventRow> =
            rows.collect::<Result<Vec<_>, _>>().map_err(MetricsError::Sqlite)?;
        match format {
            ExportFormat::Json => sonic_rs::to_string(&events).map_err(|e| {
                MetricsError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
            }),
            ExportFormat::Csv => Ok(events_to_csv(&events)),
            ExportFormat::Prometheus => Ok(events_to_prometheus(&events)),
            ExportFormat::Otel => Ok(events_to_otel(&events)),
        }
    }

    /// Delete all recorded events (keeps the `meta` table / schema). Shrinks the
    /// file via `VACUUM`.
    pub fn reset(&self) -> Result<(), MetricsError> {
        self.conn.execute("DELETE FROM events", []).map_err(MetricsError::Sqlite)?;
        self.conn.execute("VACUUM", []).map_err(MetricsError::Sqlite)?;
        Ok(())
    }
}

fn leaderboard(
    store: &Store,
    opts: &QueryOpts<'_>,
    top_n: u32,
    column: &str,
) -> Result<Vec<TopFile>, MetricsError> {
    let col = match opts.by {
        Metric::Tokens => "tokens_saved",
        Metric::Bytes => "saved_bytes",
    };
    let f = filter_clause(opts);
    let where_sql = if f.where_sql().is_empty() {
        format!("WHERE {column} IS NOT NULL")
    } else {
        format!("WHERE {column} IS NOT NULL AND {}", f.clause())
    };
    let sql = format!(
        "SELECT COALESCE({column},'<unknown>'), COALESCE(SUM({col}),0), \
         COALESCE(SUM(tokens_saved),0), COUNT(*) \
         FROM events {where} GROUP BY {column} ORDER BY SUM({col}) DESC LIMIT ?",
        column = column,
        col = col,
        where = where_sql,
    );
    let mut binds = f.binds;
    binds.push(rusqlite::types::Value::Integer(i64::from(top_n)));
    let mut stmt = store.conn.prepare(&sql).map_err(MetricsError::Sqlite)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(binds), |row| {
            Ok(TopFile {
                label: row.get(0)?,
                saved_bytes: row.get::<_, i64>(1)?.max(0) as u64,
                tokens_saved: row.get::<_, i64>(2)?.max(0) as u64,
                events: row.get::<_, i64>(3)?.max(0) as u64,
            })
        })
        .map_err(MetricsError::Sqlite)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(MetricsError::Sqlite)
}

fn event_row_from(row: &rusqlite::Row<'_>) -> Result<EventRow, rusqlite::Error> {
    Ok(EventRow {
        ts: row.get(0)?,
        day: row.get(1)?,
        kind: row.get(2)?,
        surface: row.get(3)?,
        project_id: row.get(4)?,
        source_label: row.get(5)?,
        doc_type: row.get(6)?,
        input_bytes: (row.get::<_, i64>(7)?).max(0) as u64,
        output_bytes: (row.get::<_, i64>(8)?).max(0) as u64,
        saved_bytes: (row.get::<_, i64>(9)?).max(0) as u64,
        tokens_saved: (row.get::<_, i64>(10)?).max(0) as u64,
        converted: row.get::<_, i64>(11)? != 0,
        precise: row.get::<_, i64>(12)? != 0,
    })
}

#[allow(clippy::manual_unwrap_or_default, clippy::manual_unwrap_or)]
fn events_to_csv(events: &[EventRow]) -> String {
    use std::fmt::Write;
    let mut s = String::from(
        "ts,day,kind,surface,project_id,source_label,doc_type,input_bytes,output_bytes,saved_bytes,tokens_saved,converted,precise\n",
    );
    for e in events {
        let label = match e.source_label.as_deref() {
            Some(v) => v,
            None => "",
        };
        let pid = match e.project_id.as_deref() {
            Some(v) => v,
            None => "",
        };
        let dt = match e.doc_type.as_deref() {
            Some(v) => v,
            None => "",
        };
        let label_csv = if label.contains(',') || label.contains('"') {
            format!("\"{}\"", label.replace('"', "\"\""))
        } else {
            label.to_string()
        };
        let _ = writeln!(
            s,
            "{},{},{},{},{},{},{},{},{},{},{},{},{}",
            e.ts,
            e.day,
            e.kind,
            e.surface,
            pid,
            label_csv,
            dt,
            e.input_bytes,
            e.output_bytes,
            e.saved_bytes,
            e.tokens_saved,
            i32::from(e.converted),
            i32::from(e.precise),
        );
    }
    s
}

fn escape_prometheus_label(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

fn events_to_prometheus(events: &[EventRow]) -> String {
    use std::fmt::Write;

    let mut s = String::new();
    let _ = writeln!(
        s,
        "# TYPE tooned_conversion_saved_bytes gauge\n# HELP tooned_conversion_saved_bytes Bytes saved by TOON conversion"
    );
    let _ = writeln!(
        s,
        "# TYPE tooned_conversion_tokens_saved gauge\n# HELP tooned_conversion_tokens_saved Tokens saved by TOON conversion"
    );
    let _ = writeln!(
        s,
        "# TYPE tooned_conversion_input_bytes gauge\n# HELP tooned_conversion_input_bytes Input bytes processed"
    );
    let _ = writeln!(
        s,
        "# TYPE tooned_conversion_output_bytes gauge\n# HELP tooned_conversion_output_bytes Output bytes after conversion"
    );

    for e in events {
        let ts_ms = e.ts * 1000;
        let surface = escape_prometheus_label(&e.surface);
        let kind = escape_prometheus_label(&e.kind);
        let project_id = e.project_id.as_deref().map_or_else(String::new, escape_prometheus_label);
        let source_label =
            e.source_label.as_deref().map_or_else(String::new, escape_prometheus_label);
        let doc_type = e.doc_type.as_deref().map_or_else(String::new, escape_prometheus_label);
        let _ = writeln!(
            s,
            "tooned_conversion_saved_bytes{{surface=\"{surface}\",kind=\"{kind}\",project_id=\"{project_id}\",source_label=\"{source_label}\",doc_type=\"{doc_type}\"}} {} {ts_ms}",
            e.saved_bytes
        );
        let _ = writeln!(
            s,
            "tooned_conversion_tokens_saved{{surface=\"{surface}\",kind=\"{kind}\",project_id=\"{project_id}\",source_label=\"{source_label}\",doc_type=\"{doc_type}\"}} {} {ts_ms}",
            e.tokens_saved
        );
        let _ = writeln!(
            s,
            "tooned_conversion_input_bytes{{surface=\"{surface}\",kind=\"{kind}\",project_id=\"{project_id}\",source_label=\"{source_label}\",doc_type=\"{doc_type}\"}} {} {ts_ms}",
            e.input_bytes
        );
        let _ = writeln!(
            s,
            "tooned_conversion_output_bytes{{surface=\"{surface}\",kind=\"{kind}\",project_id=\"{project_id}\",source_label=\"{source_label}\",doc_type=\"{doc_type}\"}} {} {ts_ms}",
            e.output_bytes
        );
    }
    s
}

fn events_to_otel(events: &[EventRow]) -> String {
    use std::fmt::Write;

    let mut s = String::new();
    for e in events {
        let resource_service = escape_json_string("tooned");
        let scope_name = escape_json_string("tooned-metrics");
        let name = escape_json_string("tooned.conversion");
        let description = escape_json_string("TOON conversion event");
        let surface = escape_json_string(&e.surface);
        let kind = escape_json_string(&e.kind);
        let project_id = e.project_id.as_deref().map_or_else(String::new, escape_json_string);
        let source_label = e.source_label.as_deref().map_or_else(String::new, escape_json_string);
        let doc_type = e.doc_type.as_deref().map_or_else(String::new, escape_json_string);
        let _ = writeln!(
            s,
            "{{\"resource\":{{\"service.name\":\"{resource_service}\"}},\"scope\":{{\"name\":\"{scope_name}\"}},\"metric\":{{\"name\":\"{name}\",\"description\":\"{description}\",\"unit\":\"1\",\"data\":{{\"points\":[{{\"attributes\":{{\"surface\":\"{surface}\",\"kind\":\"{kind}\",\"project_id\":\"{project_id}\",\"source_label\":\"{source_label}\",\"doc_type\":\"{doc_type}\",\"converted\":{},\"precise\":{}}},\"time_unix_nano\":{},\"as_double\":{{\"saved_bytes\":{},\"tokens_saved\":{},\"input_bytes\":{},\"output_bytes\":{}}}}}]}}}}}}",
            e.converted,
            e.precise,
            e.ts * 1_000_000_000,
            e.saved_bytes,
            e.tokens_saved,
            e.input_bytes,
            e.output_bytes
        );
    }
    s
}

fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Assign intensity levels (0..=4) across the present (non-zero) values using
/// the 20/40/60/80 percentiles, so the heatmap distributes color well
/// regardless of the absolute scale.
fn assign_levels(cells: &mut [HeatmapCell]) {
    let mut present: Vec<u64> = cells.iter().map(|c| c.value).filter(|v| *v > 0).collect();
    if present.is_empty() {
        return;
    }
    present.sort_unstable();
    let pct = |q: usize| -> u64 {
        if present.is_empty() {
            return 0;
        }
        let idx = ((present.len() * q).div_ceil(100)).saturating_sub(1).min(present.len() - 1);
        if let Some(v) = present.get(idx) { *v } else { 0 }
    };
    let t1 = pct(20);
    let t2 = pct(40);
    let t3 = pct(60);
    let t4 = pct(80);
    for cell in cells.iter_mut() {
        if cell.value == 0 {
            cell.level = 0;
        } else if cell.value >= t4 {
            cell.level = 4;
        } else if cell.value >= t3 {
            cell.level = 3;
        } else if cell.value >= t2 {
            cell.level = 2;
        } else if cell.value >= t1 {
            cell.level = 1;
        } else {
            cell.level = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// Query helpers
// ---------------------------------------------------------------------------

struct Filter {
    clause: String,
    binds: Vec<rusqlite::types::Value>,
}

impl Filter {
    fn clause(&self) -> &str {
        if self.clause.is_empty() { "1=1" } else { self.clause.as_str() }
    }

    fn where_sql(&self) -> String {
        if self.clause.is_empty() { String::new() } else { format!("WHERE {}", self.clause) }
    }
}

fn filter_clause(opts: &QueryOpts<'_>) -> Filter {
    let mut clauses = Vec::new();
    let mut binds = Vec::new();
    if !opts.include_opportunity {
        clauses.push("kind = ?".to_string());
        binds.push(rusqlite::types::Value::Text("actual".into()));
    }
    if let Some(surface) = opts.surface {
        clauses.push("surface = ?".to_string());
        binds.push(rusqlite::types::Value::Text(surface.to_string()));
    }
    // Apply the requested date window to every metrics query (summary,
    // per_surface, leaderboard, daily_aggregates). Without this, --since /
    // --until are ignored and the whole history is aggregated.
    let (since, until) = window(opts);
    clauses.push("day BETWEEN ? AND ?".to_string());
    binds.push(rusqlite::types::Value::Integer(since));
    binds.push(rusqlite::types::Value::Integer(until));
    Filter { clause: if clauses.is_empty() { String::new() } else { clauses.join(" AND ") }, binds }
}

fn window(opts: &QueryOpts<'_>) -> (i64, i64) {
    let until = opts.until_day.unwrap_or_else(today_day);
    let since = opts.since_day.unwrap_or_else(|| until.saturating_sub(364));
    (since, until)
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    let exists: Result<bool, _> = conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?1",
        [name],
        |_| Ok(true),
    );
    matches!(exists, Ok(true))
}

// ---------------------------------------------------------------------------
// Date helpers (Hinnant civil<->days, no chrono dependency)
// ---------------------------------------------------------------------------

/// `(year, month, day)` -> `i64` days-since-epoch.
pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 }.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * if m > 2 { m - 3 } else { m + 9 } + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `i64` days-since-epoch -> `(year, month, day)`.
pub(crate) fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Format a day-since-epoch as `YYYY-MM-DD`.
pub fn day_to_ymd(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Parse `YYYY-MM-DD` into a day-since-epoch. Returns `None` on any malformed
/// component (never panics on untrusted CLI input). Requires the strict fixed
/// width form and rejects impossible dates by round-tripping.
#[allow(clippy::many_single_char_names)]
pub fn ymd_to_day(s: &str) -> Option<i64> {
    if s.len() != 10 {
        return None;
    }
    let bytes = s.as_bytes();
    let dash1 = *bytes.get(4)?;
    let dash2 = *bytes.get(7)?;
    if dash1 != b'-' || dash2 != b'-' {
        return None;
    }
    let mut digits_ok = true;
    for i in [0usize, 1, 2, 3, 5, 6, 8, 9] {
        match bytes.get(i) {
            Some(v) if v.is_ascii_digit() => {}
            _ => digits_ok = false,
        }
    }
    if !digits_ok {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day_of_month: i64 = s.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day_of_month) {
        return None;
    }
    let day_num = days_from_civil(year, month, day_of_month);
    if day_to_ymd(day_num) != s {
        return None;
    }
    Some(day_num)
}

fn day_of_ts(ts: i64) -> i64 {
    (ts / 86_400) + (if ts < 0 && ts % 86_400 != 0 { -1 } else { 0 })
}

/// Current day-since-epoch (UTC).
pub fn today_day() -> i64 {
    day_of_ts(now_unix())
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// Resolve the user-global metrics database path. Honors `TOONED_METRICS_DIR`
/// (tests + power users) and `XDG_DATA_HOME` first, then falls back to the
/// platform data directory. The file is always `<dir>/metrics.db`.
pub fn user_global_db_path() -> PathBuf {
    if let Ok(dir) = env::var("TOONED_METRICS_DIR") {
        return PathBuf::from(dir).join("metrics.db");
    }
    if let Ok(dir) = env::var("XDG_DATA_HOME") {
        let mut p = PathBuf::from(dir);
        p.push("tooned");
        p.push("metrics.db");
        return p;
    }
    if let Ok(home) = env::var("HOME") {
        let mut p = PathBuf::from(home);
        p.push(".local");
        p.push("share");
        p.push("tooned");
        p.push("metrics.db");
        return p;
    }
    PathBuf::from(".tooned-metrics.db")
}

/// Resolve the project-scoped metrics database path: `<root>/.tooned/metrics.db`.
pub fn project_db_path(root: &Path) -> PathBuf {
    let mut p = root.to_path_buf();
    p.push(".tooned");
    p.push("metrics.db");
    p
}

/// Reject paths that are symlinks. The metrics database path is either
/// caller-supplied or derived from `TOONED_METRICS_DIR` / `XDG_DATA_HOME`, so
/// a pre-placed symlink could otherwise redirect reads/writes to an arbitrary
/// location.
fn refuse_symlink(path: &Path, label: &str) -> Result<(), MetricsError> {
    if let Ok(meta) = std::fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        return Err(MetricsError::Symlink(std::path::PathBuf::from(format!(
            "{label} at {}",
            path.display()
        ))));
    }
    Ok(())
}

fn ensure_parent(db_path: &Path) -> Result<(), MetricsError> {
    if let Some(parent) = db_path.parent() {
        refuse_symlink(parent, "metrics database directory")?;
        #[cfg(unix)]
        let parent_existed = parent.exists();
        std::fs::create_dir_all(parent).map_err(MetricsError::Io)?;
        // Re-check after creating the directory in case a TOCTOU swap occurred
        // (best-effort; the earlier check already stops the common case).
        refuse_symlink(parent, "metrics database directory")?;
        // Only chmod the parent if we just created it; do not alter permissions
        // on an existing directory that may be shared (e.g. /tmp or a system dir).
        #[cfg(unix)]
        if !parent_existed {
            set_mode(parent, 0o700);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(mut perms) = std::fs::metadata(path).map(|m| m.permissions()) {
        perms.set_mode(mode);
        let _ = std::fs::set_permissions(path, perms);
    }
}

// ---------------------------------------------------------------------------
// Recording helpers (best-effort; never panic, never fail the caller)
// ---------------------------------------------------------------------------

/// True when metrics recording is disabled via `TOONED_METRICS_DISABLE`.
pub fn metrics_disabled() -> bool {
    env::var_os("TOONED_METRICS_DISABLE").is_some()
}

/// Opaque, salted project id from an absolute project-root path.
pub fn project_id(root: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(PROJECT_ID_SALT.as_bytes());
    hasher.update(root.to_string_lossy().as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Record `event` to the user-global ledger. Swallows every error and never
/// panics, so it is safe to call from the conversion hot path. No-op when
/// metrics are disabled.
pub fn record_event(event: &Event) {
    if metrics_disabled() {
        return;
    }
    let path = user_global_db_path();
    if let Ok(store) = Store::open(&path) {
        let _ = store.record(event);
        let _ = store.conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", []);
    }
}

/// Record `event` to a project-scoped ledger at `<root>/.tooned/metrics.db`.
/// Swallows every error; safe on the hot path. No-op when disabled.
pub fn record_event_project(event: &Event, root: &Path) {
    if metrics_disabled() {
        return;
    }
    let path = project_db_path(root);
    if let Ok(store) = Store::open(&path) {
        let _ = store.record(event);
        let _ = store.conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", []);
    }
}

/// Record `event` to both the global ledger and, when `project_root` is
/// `Some`, the project-scoped ledger. Convenience for CLI/hook callers.
pub fn record_event_all(event: &Event, project_root: Option<&Path>) {
    record_event(event);
    if let Some(root) = project_root {
        record_event_project(event, root);
    }
}

/// Current Unix timestamp (seconds), clamped to `0` if the clock predates the
/// epoch -- never a panic on a clock read failure.
#[allow(clippy::cast_possible_wrap)]
pub fn now_unix() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => 0,
    }
}

/// Estimate tokens saved from bytes saved using the default 4-bytes/token rule.
pub fn estimate_tokens(saved_bytes: u64) -> u64 {
    saved_bytes / BYTES_PER_TOKEN
}

/// Opaque project identifier: a salted BLAKE3 hash of the project's
/// absolute path. Lets cross-project rollups work without ever persisting the
/// real (possibly sensitive) directory layout.
const PROJECT_ID_SALT: &str = "tooned-metrics:v1:project-id";

/// Default heuristic for estimating saved tokens from saved bytes (4 bytes/token
/// is the usual rule of thumb for English-ish JSON). Real BPE counting is a
/// future, optional (`precise`) enhancement and is never fetched over the
/// network.
const BYTES_PER_TOKEN: u64 = 4;

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn sample(
        surface: &str,
        kind: EventKind,
        input: u64,
        output: u64,
        label: Option<&str>,
    ) -> Event {
        RecordBuilder::new(surface)
            .kind(kind)
            .at(0)
            .sizes(input, output)
            .converted(kind == EventKind::Actual)
            .source_label(label.map(str::to_string))
            .doc_type(Some("json".to_string()))
            .build()
    }

    #[test]
    fn date_round_trip_is_stable() {
        for s in ["1970-01-01", "2000-06-15", "2026-12-31", "1999-02-28", "2000-02-29"] {
            let day = ymd_to_day(s).expect("valid date");
            assert_eq!(day_to_ymd(day).as_str(), s);
        }
        assert_eq!(ymd_to_day("1970-01-01"), Some(0));
        assert_eq!(ymd_to_day("2000-01-01"), Some(10_957));
        assert_eq!(ymd_to_day("2026-07-17"), Some(20_651));
    }

    #[test]
    fn invalid_dates_are_none() {
        assert_eq!(ymd_to_day("not-a-date"), None);
        assert_eq!(ymd_to_day("2026-13-01"), None);
        assert_eq!(ymd_to_day("2026-00-01"), None);
        assert_eq!(ymd_to_day("2026-1-1"), None);
    }

    #[test]
    fn open_creates_and_counts() {
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("metrics.db");
        let store = Store::open(&db).expect("open");
        assert_eq!(store.count().expect("count"), 0);
        store
            .record(&sample("hook:claude", EventKind::Actual, 100, 40, Some("a.json")))
            .expect("record");
        assert_eq!(store.count().expect("count"), 1);
    }

    #[test]
    fn heatmap_summary_and_top() {
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("metrics.db");
        let store = Store::open(&db).expect("open");
        store
            .record(&sample("hook:claude", EventKind::Actual, 100, 40, Some("a.json")))
            .expect("r1");
        store
            .record(&sample("cli:convert", EventKind::Actual, 200, 100, Some("b.json")))
            .expect("r2");
        store
            .record(&sample("index:scan", EventKind::Opportunity, 500, 400, Some("c.json")))
            .expect("r3");

        let opts = QueryOpts {
            since_day: Some(0),
            until_day: Some(0),
            by: Metric::Bytes,
            include_opportunity: false,
            surface: None,
        };
        let cells = store.heatmap(&opts).expect("heatmap");
        assert_eq!(cells.len(), 1);
        let first = cells.first().expect("non-empty cells");
        assert_eq!(first.value, 160);
        assert_eq!(first.level, 4);

        let s = store.summary(&opts).expect("summary");
        assert_eq!(s.total_saved_bytes, 160);
        assert_eq!(s.conversions, 2);

        let mut opts2 = opts.clone();
        opts2.include_opportunity = true;
        let s2 = store.summary(&opts2).expect("summary2");
        assert_eq!(s2.total_saved_bytes, 260);

        let top = store.top_files(&opts, 10).expect("top");
        assert_eq!(top.len(), 2);
        assert_eq!(top.first().expect("non-empty top").label, "b.json");

        store.reset().expect("reset");
        assert_eq!(store.count().expect("count"), 0);
    }

    #[test]
    fn export_prometheus_and_otel() {
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("metrics.db");
        let store = Store::open(&db).expect("open");
        store
            .record(&sample("hook:claude", EventKind::Actual, 100, 40, Some("a.json")))
            .expect("record");

        let prom = store.export(ExportFormat::Prometheus, None, None).expect("prometheus");
        assert!(prom.contains("# TYPE tooned_conversion_saved_bytes gauge"));
        assert!(prom.contains("tooned_conversion_saved_bytes"));
        assert!(prom.contains("surface=\"hook:claude\""));

        let otel = store.export(ExportFormat::Otel, None, None).expect("otel");
        assert!(otel.contains("\"name\":\"tooned.conversion\""));
        assert!(otel.contains("\"saved_bytes\":60"));
    }
}
