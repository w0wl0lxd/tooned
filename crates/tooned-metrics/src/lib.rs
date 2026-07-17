// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned-metrics`: the local-only, off-machine, opt-out metrics ledger for
//! tooned.
//!
//! Every conversion/outcome across all tooned surfaces is recorded here so the
//! CLI can render a GitHub/Codex-style token-savings heatmap and summary views.
//! The ledger is a SQLite file on local disk only; nothing is ever sent over
//! the network, and recording is best-effort (a metrics failure can never
//! change a conversion's output or exit code).

#![allow(clippy::cast_sign_loss)]

pub mod store;

pub use store::{
    Event, EventKind, EventRow, ExportFormat, HeatmapCell, Metric, MetricsError, PerSurface,
    QueryOpts, RecordBuilder, Store, Summary, TopFile, day_to_ymd, project_db_path, record_event,
    record_event_all, record_event_project, today_day, user_global_db_path, ymd_to_day,
};

/// Current on-disk schema version. Bumped (and migrated) if the `events` table
/// shape changes.
pub const SCHEMA_VERSION: &str = "1";
