//! # tooned-index
//!
//! The `.tooned/` on-disk SQLite index: directory scanning, content
//! fingerprinting, and cached shape/conversion reports, invoked on-demand by
//! `tooned index` / `tooned index sync` / `tooned stats` — never on the
//! hot hook path (see `tooned-core` for that).
//!
//! This is a scaffold: the index schema and scan/sync logic are implemented
//! following the spec-kit pipeline (`specs/`), not directly in this initial
//! commit.

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}
