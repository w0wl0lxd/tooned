//! MCP stdio server implementation (`rmcp`, `transport-io` feature).
//!
//! Every tool delegates to the exact same `tooned_core`/`tooned_index`
//! functions the CLI/hooks already call -- no parallel conversion logic
//! (constitution Principle V, `contracts/mcp-tools.md`). `tooned_convert`/
//! `tooned_detect` operate purely on the `content` string passed in the
//! tool call and never touch the filesystem (`contracts/mcp-tools.md`'s
//! explicit rule, extending constitution Principle III to this entrypoint).

// `#[tool_router(server_handler)]` (rmcp 2.2.0) expands to a `ServerHandler`
// impl containing a default async fn with no `.await` (it returns
// `std::future::ready(..)` internally). This is generated code inside the
// `rmcp`/`rmcp-macros` crates, not anything written here -- there is no
// local span to fix. Module-scoped rather than a single-item `#[allow]`
// since the lint attaches to the macro-generated impl block, not to the
// `impl ToonedMcpServer` item the attribute is literally written on.
// `unknown_lints` guards this across toolchains/clippy versions where
// `unused_async_trait_impl` isn't a registered lint at all -- without it,
// `-D warnings` turns "unknown lint" itself into a hard compile error on
// any environment that doesn't happen to have this lint (observed: it
// fired locally but was rejected as unknown by CI's identically-versioned
// clippy, most likely due to a stale sccache/kache-cached diagnostic
// locally rather than a genuine version difference).
#![allow(unknown_lints)]
#![allow(clippy::unused_async_trait_impl)]

use std::path::PathBuf;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{Json, ServiceExt, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::task;
use tooned_core::{
    Conversion, ConversionOptions, ConversionReport, DocType, InspectReport, PassthroughReason,
    ShapeClass,
};

/// Maps an MCP `format_hint` string onto `tooned_core::DocType`. Any
/// unrecognized hint is treated as "no hint" (falls back to content
/// sniffing) rather than a tool-call error -- a caller-supplied hint is
/// advisory, not something adversarial payload data ever flows through, but
/// still shouldn't be able to hard-fail a call over a typo.
fn parse_doc_type_hint(hint: Option<&str>) -> Option<DocType> {
    match hint?.to_ascii_lowercase().as_str() {
        "json" => Some(DocType::Json),
        "ndjson" | "jsonl" => Some(DocType::NdJson),
        "yaml" | "yml" => Some(DocType::Yaml),
        "toml" => Some(DocType::Toml),
        "csv" => Some(DocType::Csv),
        "tsv" => Some(DocType::Tsv),
        "xml" => Some(DocType::Xml),
        _ => None,
    }
}

/// Resolves and validates a client-supplied `path` for the three
/// filesystem-touching MCP tools below (`tooned_index_build`,
/// `tooned_index_refresh`, `tooned_stats`), which otherwise take `path`
/// with no validation, allow-list, or sandboxing at all -- driving a
/// recursive directory walk (content-hashing every reachable file, per
/// `tooned_index::scan::MAX_SCAN_ENTRIES`'s own bound on the walk's raw
/// size) plus writes (`.tooned/index.db`, a `.gitignore` append) rooted
/// wherever it points. Unlike the CLI (typed by a human), an MCP tool call
/// is typically issued autonomously by the agent -- including in response
/// to content the agent doesn't fully control (a classic prompt-injection
/// vector) -- so `path` must not be trusted unconditionally.
///
/// A full project-root allow-list isn't viable here without a new
/// configuration surface (the whole point of these tools is to accept an
/// arbitrary project directory, which routinely differs from wherever the
/// server process happens to have been started), so this instead refuses
/// the single highest-blast-radius case explicitly named in the threat
/// scenario this guards against: `path` resolving (after canonicalization,
/// so a symlink or `..` can't disguise it) to the filesystem root or to the
/// resolved user's exact home directory -- the two roots under which a
/// full recursive content-hashing walk plus a `.gitignore`/index-db write
/// would be most damaging. Legitimate project subdirectories (including
/// ones nested under the home directory, which is the overwhelmingly common
/// case) are unaffected.
fn resolve_index_path(raw_path: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(raw_path);
    if !candidate.is_dir() {
        return Err(format!("path not found or not a directory: {raw_path}"));
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|err| format!("could not resolve path {raw_path:?}: {err}"))?;

    let mut denied_roots: Vec<PathBuf> = Vec::new();
    // The filesystem root itself (`/`, or a drive root on Windows).
    if let Some(root) = canonical.ancestors().last() {
        denied_roots.push(root.to_path_buf());
    }
    if let Some(home) = home_dir()
        && let Ok(canonical_home) = home.canonicalize()
    {
        denied_roots.push(canonical_home);
    }

    if denied_roots.iter().any(|denied| denied == &canonical) {
        return Err(format!(
            "refusing to index {raw_path:?}: it resolves to {}, which is the filesystem root or \
             the user's home directory -- point this tool at a specific project directory \
             instead",
            canonical.display()
        ));
    }

    Ok(canonical)
}

fn home_dir() -> Option<PathBuf> {
    for var in ["HOME", "USERPROFILE"] {
        if let Some(v) = std::env::var_os(var)
            && !v.is_empty()
        {
            return Some(PathBuf::from(v));
        }
    }
    None
}

fn build_options(format_hint: Option<&str>, margin_pct: Option<f64>) -> ConversionOptions {
    let margin_pct = match margin_pct {
        Some(m) => m,
        None => ConversionOptions::default().margin_pct,
    };
    ConversionOptions {
        format_hint: parse_doc_type_hint(format_hint),
        margin_pct,
        ..ConversionOptions::default()
    }
}

/// Structured mirror of `tooned_core::DocType`, so MCP JSON consumers get
/// a stable, typed value instead of Rust's `#[derive(Debug)]` formatting
/// (finding: a `format!("{dt:?}")` string is opaque and silently changes
/// shape on any future rename/refactor of the underlying enum, with no
/// version guard).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DocTypeDto {
    Json,
    NdJson,
    Yaml,
    Toml,
    Csv,
    Tsv,
    Xml,
}

impl From<DocType> for DocTypeDto {
    fn from(doc_type: DocType) -> Self {
        match doc_type {
            DocType::Json => Self::Json,
            DocType::NdJson => Self::NdJson,
            DocType::Yaml => Self::Yaml,
            DocType::Toml => Self::Toml,
            DocType::Csv => Self::Csv,
            DocType::Tsv => Self::Tsv,
            DocType::Xml => Self::Xml,
        }
    }
}

/// Structured mirror of `tooned_core::ShapeClass` (see [`DocTypeDto`]'s
/// doc comment for why this replaces a Debug-formatted string) --
/// preserves `uniformity_pct`/`sampled` as real numeric fields rather than
/// embedding them in an opaque string.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShapeClassDto {
    UniformArrayOfObjects { uniformity_pct: f64, sampled: usize },
    Irregular,
    Scalar,
}

impl From<ShapeClass> for ShapeClassDto {
    fn from(shape: ShapeClass) -> Self {
        match shape {
            ShapeClass::UniformArrayOfObjects { uniformity_pct, sampled } => {
                Self::UniformArrayOfObjects { uniformity_pct, sampled }
            }
            ShapeClass::Irregular => Self::Irregular,
            ShapeClass::Scalar => Self::Scalar,
        }
    }
}

/// Structured mirror of `tooned_core::PassthroughReason` (see
/// [`DocTypeDto`]'s doc comment for why this replaces a Debug-formatted
/// string) -- preserves `NotSmallerEnough`'s `json_bytes`/`toon_bytes` as
/// real numeric fields a client can branch/report on directly, rather than
/// only being recoverable by parsing Rust's Debug text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PassthroughReasonDto {
    NotStructuredData,
    ParseFailed,
    InputTooLarge,
    NotSmallerEnough { json_bytes: usize, toon_bytes: usize },
    RoundTripMismatch,
}

impl From<PassthroughReason> for PassthroughReasonDto {
    fn from(reason: PassthroughReason) -> Self {
        match reason {
            PassthroughReason::NotStructuredData => Self::NotStructuredData,
            PassthroughReason::ParseFailed => Self::ParseFailed,
            PassthroughReason::InputTooLarge => Self::InputTooLarge,
            PassthroughReason::NotSmallerEnough { json_bytes, toon_bytes } => {
                Self::NotSmallerEnough { json_bytes, toon_bytes }
            }
            PassthroughReason::RoundTripMismatch => Self::RoundTripMismatch,
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ConvertRequest {
    /// Raw document content to (maybe) convert.
    pub content: String,
    /// Optional doc-type hint: "json"/"ndjson"/"yaml"/"toml"/"csv"/"tsv".
    #[serde(default)]
    pub format_hint: Option<String>,
    /// Overrides the default 2% adaptive-savings margin.
    #[serde(default)]
    pub margin_pct: Option<f64>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ConversionReportDto {
    pub doc_type: DocTypeDto,
    pub shape: ShapeClassDto,
    pub json_bytes: usize,
    pub toon_bytes: usize,
    pub savings_pct: f64,
}

impl From<ConversionReport> for ConversionReportDto {
    fn from(report: ConversionReport) -> Self {
        Self {
            doc_type: report.doc_type.into(),
            shape: report.shape.into(),
            json_bytes: report.json_bytes,
            toon_bytes: report.toon_bytes,
            savings_pct: report.savings_pct,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ConvertResult {
    pub converted: bool,
    pub text: String,
    pub report: Option<ConversionReportDto>,
    /// Populated whenever `converted` is `false`, so a caller can learn why
    /// this call declined to convert (`NotStructuredData`/`ParseFailed`/
    /// `InputTooLarge`/`NotSmallerEnough`/`RoundTripMismatch`) without a
    /// second `tooned_detect` round-trip on the same content.
    pub reason: Option<PassthroughReasonDto>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DetectRequest {
    pub content: String,
    #[serde(default)]
    pub format_hint: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DetectResult {
    pub doc_type: Option<DocTypeDto>,
    pub shape: ShapeClassDto,
    pub input_bytes: usize,
    pub json_bytes: Option<usize>,
    pub toon_bytes: Option<usize>,
    pub savings_pct: Option<f64>,
    pub would_convert: bool,
    pub reason: Option<PassthroughReasonDto>,
}

impl From<InspectReport> for DetectResult {
    fn from(report: InspectReport) -> Self {
        Self {
            doc_type: report.doc_type.map(DocTypeDto::from),
            shape: report.shape.into(),
            input_bytes: report.input_bytes,
            json_bytes: report.json_bytes,
            toon_bytes: report.toon_bytes,
            savings_pct: report.savings_pct,
            would_convert: report.would_convert,
            reason: report.reason.map(PassthroughReasonDto::from),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DecodeRequest {
    pub toon: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DecodeResult {
    pub value: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IndexPathRequest {
    pub path: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct IndexBuildResult {
    pub files_scanned: usize,
    pub gitignore_updated: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct IndexRefreshResult {
    pub files_rescanned: usize,
    pub files_pruned: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StatsRequest {
    pub path: String,
    #[serde(default)]
    pub top_n: Option<u32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct StatsEntry {
    pub path: String,
    pub savings_pct: f64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct StatsResult {
    pub results: Vec<StatsEntry>,
}

/// The `tooned mcp serve` handler: a thin, agent-agnostic MCP wrapper over
/// `tooned_core`/`tooned_index`'s existing public API. Holds no state of
/// its own -- `#[tool_router(server_handler)]`'s default dispatch builds a
/// fresh `ToolRouter` per call via the generated `Self::tool_router()`
/// rather than reading a stored field, so no field is needed here.
#[derive(Debug, Clone, Copy, Default)]
pub struct ToonedMcpServer;

impl ToonedMcpServer {
    pub fn new() -> Self {
        Self
    }
}

#[tool_router(server_handler)]
impl ToonedMcpServer {
    #[tool(description = "Adaptively convert content to TOON when it is measurably smaller than \
                        compact JSON (per the same margin/round-trip rules as the CLI and \
                        hooks); otherwise returns the content unchanged. Operates purely on the \
                        content string -- never reads or writes the filesystem.")]
    async fn tooned_convert(
        &self,
        Parameters(req): Parameters<ConvertRequest>,
    ) -> Result<Json<ConvertResult>, String> {
        // The core conversion is CPU-bound and delegates to a third-party
        // codec; running it off the async executor and on a blocking thread
        // both prevents the Tokio runtime from being monopolised and ensures
        // any panic in the codec path is caught as a `JoinError` instead of
        // killing the entire MCP server process.
        task::spawn_blocking(move || {
            let opts = build_options(req.format_hint.as_deref(), req.margin_pct);
            match tooned_core::maybe_tooned(req.content.as_bytes(), &opts) {
                Ok(Conversion::Toon { text, report }) => Json(ConvertResult {
                    converted: true,
                    text,
                    report: Some(report.into()),
                    reason: None,
                }),
                // `attempt()`/`maybe_tooned` already computed exactly why this
                // declined to convert -- surfaced here rather than discarded.
                Ok(Conversion::Passthrough { reason, .. }) => Json(ConvertResult {
                    converted: false,
                    text: req.content,
                    report: None,
                    reason: Some(reason.into()),
                }),
                // Infallible in practice; a genuine caller-misuse Err still
                // falls back to the fail-safe passthrough shape.
                Err(_) => Json(ConvertResult {
                    converted: false,
                    text: req.content,
                    report: None,
                    reason: None,
                }),
            }
        })
        .await
        .map_err(|err| err.to_string())
    }

    #[tool(description = "Dry-run doc-type/shape/estimated-savings detection with no conversion \
                        performed. Operates purely on the content string -- never reads or \
                        writes the filesystem.")]
    async fn tooned_detect(
        &self,
        Parameters(req): Parameters<DetectRequest>,
    ) -> Result<Json<DetectResult>, String> {
        task::spawn_blocking(move || {
            let opts = build_options(req.format_hint.as_deref(), None);
            let report = tooned_core::inspect(req.content.as_bytes(), &opts);
            Json(report.into())
        })
        .await
        .map_err(|err| err.to_string())
    }

    #[tool(description = "Decode a TOON document back into a structured JSON value.")]
    async fn tooned_decode(
        &self,
        Parameters(req): Parameters<DecodeRequest>,
    ) -> Result<Json<DecodeResult>, String> {
        let value = task::spawn_blocking(move || tooned_core::decode_toon(&req.toon))
            .await
            .map_err(|err| err.to_string())?;
        value.map(|v| Json(DecodeResult { value: v })).map_err(|err| err.to_string())
    }

    #[tool(description = "Full scan + classify a project directory into its .tooned/ index, \
                        creating the index (and .gitignore entry) if this is the first build.")]
    async fn tooned_index_build(
        &self,
        Parameters(req): Parameters<IndexPathRequest>,
    ) -> Result<Json<IndexBuildResult>, String> {
        let result = task::spawn_blocking(move || {
            let root = resolve_index_path(&req.path)?;
            // `gitignore_updated` mirrors data-model.md's rule: the append only
            // ever happens on the index's first creation for a project, so
            // "no index existed yet before this call" is the accurate signal.
            let existed_before = tooned_index::index_exists(&root);
            let summary = tooned_index::scan_full(&root).map_err(|err| err.to_string())?;
            Ok::<IndexBuildResult, String>(IndexBuildResult {
                files_scanned: summary.files_scanned,
                gitignore_updated: !existed_before,
            })
        })
        .await
        .map_err(|err| err.to_string())?;
        result.map(Json)
    }

    #[tool(description = "Incremental refresh of an existing .tooned/ index: re-scans changed \
                        files and prunes deleted ones. Requires a prior tooned_index_build.")]
    async fn tooned_index_refresh(
        &self,
        Parameters(req): Parameters<IndexPathRequest>,
    ) -> Result<Json<IndexRefreshResult>, String> {
        let result = task::spawn_blocking(move || {
            let root = resolve_index_path(&req.path)?;
            let summary = tooned_index::sync(&root).map_err(|err| err.to_string())?;
            Ok::<IndexRefreshResult, String>(IndexRefreshResult {
                files_rescanned: summary.added + summary.updated,
                files_pruned: summary.removed,
            })
        })
        .await
        .map_err(|err| err.to_string())?;
        result.map(Json)
    }

    #[tool(description = "Ranked savings-opportunity report from an existing .tooned/ index, \
                        ordered by savings_pct descending.")]
    async fn tooned_stats(
        &self,
        Parameters(req): Parameters<StatsRequest>,
    ) -> Result<Json<StatsResult>, String> {
        let result = task::spawn_blocking(move || {
            let root = resolve_index_path(&req.path)?;
            let rows = tooned_index::stats(&root, req.top_n).map_err(|err| err.to_string())?;
            Ok::<StatsResult, String>(StatsResult {
                results: rows
                    .into_iter()
                    .map(|row| StatsEntry { path: row.path, savings_pct: row.savings_pct })
                    .collect(),
            })
        })
        .await
        .map_err(|err| err.to_string())?;
        result.map(Json)
    }
}

async fn serve_async() -> anyhow::Result<()> {
    let server = ToonedMcpServer::new().serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}

/// Runs the MCP server over stdio until the transport closes. Errors here
/// are transport-level startup failures only (`contracts/cli.md`: `mcp
/// serve` exits non-zero only in that case) -- tool-call-level failures
/// never propagate out of an individual tool handler as a process error.
pub fn serve() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(serve_async())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_doc_type_hint_round_trips_all_format_hints() {
        let cases = [
            ("json", Some(DocType::Json)),
            ("JSON", Some(DocType::Json)),
            ("ndjson", Some(DocType::NdJson)),
            ("jsonl", Some(DocType::NdJson)),
            ("yaml", Some(DocType::Yaml)),
            ("yml", Some(DocType::Yaml)),
            ("toml", Some(DocType::Toml)),
            ("csv", Some(DocType::Csv)),
            ("tsv", Some(DocType::Tsv)),
            ("xml", Some(DocType::Xml)),
            ("XML", Some(DocType::Xml)),
            ("unknown", None),
            ("", None),
        ];

        for (hint, expected) in cases {
            assert_eq!(
                parse_doc_type_hint(Some(hint)),
                expected,
                "parse_doc_type_hint({hint:?}) should return {expected:?}"
            );
        }
        assert_eq!(parse_doc_type_hint(None), None, "no hint is treated as None");
    }
}
