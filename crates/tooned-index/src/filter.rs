// SPDX-License-Identifier: AGPL-3.0-only

//! Scan/filter helpers for `tooned index` -- type and exclude constraints.

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use tooned_types::DocType;

/// Filters that can be applied to `tooned index`, `tooned index sync`, and
/// `tooned stats`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexFilter {
    /// Only include files whose detected document type matches this value.
    /// `None` means "any recognized or unrecognized type".
    pub type_filter: Option<DocTypeFilter>,
    /// Paths matching any of these gitignore-style globs are skipped.
    pub excludes: Vec<String>,
}

/// Document-type filter values accepted by the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocTypeFilter {
    Json,
    NdJson,
    Yaml,
    Toml,
    Csv,
    Tsv,
    Xml,
    Msgpack,
    Cbor,
    Json5,
    /// Files that are not recognized as any structured document type.
    Bin,
}

impl DocTypeFilter {
    /// Parses a CLI `--type` argument.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "json" => Some(Self::Json),
            "ndjson" => Some(Self::NdJson),
            "yaml" => Some(Self::Yaml),
            "toml" => Some(Self::Toml),
            "csv" => Some(Self::Csv),
            "tsv" => Some(Self::Tsv),
            "xml" => Some(Self::Xml),
            "msgpack" => Some(Self::Msgpack),
            "cbor" => Some(Self::Cbor),
            "json5" => Some(Self::Json5),
            "bin" => Some(Self::Bin),
            _ => None,
        }
    }

    /// Returns the string label used in CLI help and the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::NdJson => "ndjson",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Xml => "xml",
            Self::Msgpack => "msgpack",
            Self::Cbor => "cbor",
            Self::Json5 => "json5",
            Self::Bin => "bin",
        }
    }

    /// Returns the corresponding structured `DocType`, if any.
    pub fn as_doc_type(self) -> Option<DocType> {
        match self {
            Self::Json => Some(DocType::Json),
            Self::NdJson => Some(DocType::NdJson),
            Self::Yaml => Some(DocType::Yaml),
            Self::Toml => Some(DocType::Toml),
            Self::Csv => Some(DocType::Csv),
            Self::Tsv => Some(DocType::Tsv),
            Self::Xml => Some(DocType::Xml),
            Self::Msgpack => Some(DocType::Msgpack),
            Self::Cbor => Some(DocType::Cbor),
            Self::Json5 => Some(DocType::Json5),
            Self::Bin => None,
        }
    }

    /// Does this filter match a detected `DocType`? `None` matches only `Bin`.
    pub fn matches(self, doc_type: Option<DocType>) -> bool {
        match (self, doc_type) {
            (Self::Bin, None) => true,
            (_, None) => false,
            (_, Some(dt)) => self.as_doc_type() == Some(dt),
        }
    }

    /// Does this filter match a `doc_type` string stored in the index? `None`
    /// matches only `Bin`.
    pub fn matches_str(self, doc_type: Option<&str>) -> bool {
        match (self, doc_type) {
            (Self::Bin, None) => true,
            (_, None) => false,
            (_, Some(s)) => self.as_str() == s,
        }
    }
}

impl IndexFilter {
    /// True when no type or exclude constraint is present.
    pub fn is_empty(&self) -> bool {
        self.type_filter.is_none() && self.excludes.is_empty()
    }

    /// Does `doc_type` satisfy the type filter? Always true when no filter is set.
    pub fn matches_type(&self, doc_type: Option<DocType>) -> bool {
        match self.type_filter {
            Some(f) => f.matches(doc_type),
            None => true,
        }
    }

    /// Does `doc_type_str` satisfy the type filter?
    pub fn matches_type_str(&self, doc_type: Option<&str>) -> bool {
        match self.type_filter {
            Some(f) => f.matches_str(doc_type),
            None => true,
        }
    }

    /// Build an `ignore::Gitignore` from the exclude globs, rooted at `root`.
    pub fn compile_excludes(&self, root: &Path) -> Result<Gitignore, ignore::Error> {
        let mut builder = GitignoreBuilder::new(root);
        for pattern in &self.excludes {
            builder.add_line(None, pattern)?;
        }
        builder.build()
    }

    /// True when `path` (relative to `root`) matches an exclude glob. Non-UTF8
    /// paths are conservatively treated as not excluded.
    pub fn is_excluded(&self, path: &Path, root: &Path, gitignore: &Gitignore) -> bool {
        let Ok(rel) = path.strip_prefix(root) else {
            return false;
        };
        let Some(rel_str) = rel.to_str() else {
            return false;
        };
        let is_dir = std::fs::metadata(path).is_ok_and(|m| m.is_dir());
        gitignore.matched(rel_str, is_dir).is_ignore()
    }

    /// Utility: build the gitignore and check exclusion in one call.
    pub fn path_excluded(&self, path: &Path, root: &Path) -> bool {
        match self.compile_excludes(root) {
            Ok(g) => self.is_excluded(path, root, &g),
            Err(_) => false,
        }
    }
}
