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
#![allow(clippy::unused_async_trait_impl)]

use std::path::PathBuf;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{Json, ServiceExt, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tooned_core::{Conversion, ConversionOptions, ConversionReport, DocType, InspectReport};

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
        _ => None,
    }
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
    pub doc_type: String,
    pub shape: String,
    pub json_bytes: usize,
    pub toon_bytes: usize,
    pub savings_pct: f64,
}

impl From<ConversionReport> for ConversionReportDto {
    fn from(report: ConversionReport) -> Self {
        Self {
            doc_type: format!("{:?}", report.doc_type),
            shape: format!("{:?}", report.shape),
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
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DetectRequest {
    pub content: String,
    #[serde(default)]
    pub format_hint: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DetectResult {
    pub doc_type: Option<String>,
    pub shape: String,
    pub input_bytes: usize,
    pub json_bytes: Option<usize>,
    pub toon_bytes: Option<usize>,
    pub savings_pct: Option<f64>,
    pub would_convert: bool,
    pub reason: Option<String>,
}

impl From<InspectReport> for DetectResult {
    fn from(report: InspectReport) -> Self {
        Self {
            doc_type: report.doc_type.map(|dt| format!("{dt:?}")),
            shape: format!("{:?}", report.shape),
            input_bytes: report.input_bytes,
            json_bytes: report.json_bytes,
            toon_bytes: report.toon_bytes,
            savings_pct: report.savings_pct,
            would_convert: report.would_convert,
            reason: report.reason.map(|r| format!("{r:?}")),
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
    ) -> Json<ConvertResult> {
        let opts = build_options(req.format_hint.as_deref(), req.margin_pct);
        match tooned_core::maybe_tooned(req.content.as_bytes(), &opts) {
            Ok(Conversion::Toon { text, report }) => {
                Json(ConvertResult { converted: true, text, report: Some(report.into()) })
            }
            // Infallible in practice (maybe_tooned never Errs for
            // payload-driven input); a genuine caller-misuse Err still
            // falls back to the fail-safe passthrough shape rather than a
            // protocol-level crash (constitution Principle I).
            Ok(Conversion::Passthrough { .. }) | Err(_) => {
                Json(ConvertResult { converted: false, text: req.content, report: None })
            }
        }
    }

    #[tool(description = "Dry-run doc-type/shape/estimated-savings detection with no conversion \
                        performed. Operates purely on the content string -- never reads or \
                        writes the filesystem.")]
    async fn tooned_detect(
        &self,
        Parameters(req): Parameters<DetectRequest>,
    ) -> Json<DetectResult> {
        let opts = build_options(req.format_hint.as_deref(), None);
        let report = tooned_core::inspect(req.content.as_bytes(), &opts);
        Json(report.into())
    }

    #[tool(description = "Decode a TOON document back into a structured JSON value.")]
    async fn tooned_decode(
        &self,
        Parameters(req): Parameters<DecodeRequest>,
    ) -> Result<Json<DecodeResult>, String> {
        tooned_core::decode_toon(&req.toon)
            .map(|value| Json(DecodeResult { value }))
            .map_err(|err| err.to_string())
    }

    #[tool(description = "Full scan + classify a project directory into its .tooned/ index, \
                        creating the index (and .gitignore entry) if this is the first build.")]
    async fn tooned_index_build(
        &self,
        Parameters(req): Parameters<IndexPathRequest>,
    ) -> Result<Json<IndexBuildResult>, String> {
        let root = PathBuf::from(&req.path);
        if !root.is_dir() {
            return Err(format!("path not found or not a directory: {}", req.path));
        }
        // `gitignore_updated` mirrors data-model.md's rule: the append only
        // ever happens on the index's first creation for a project, so
        // "no index existed yet before this call" is the accurate signal.
        let existed_before = tooned_index::index_exists(&root);
        let summary = tooned_index::scan_full(&root).map_err(|err| err.to_string())?;
        Ok(Json(IndexBuildResult {
            files_scanned: summary.files_scanned,
            gitignore_updated: !existed_before,
        }))
    }

    #[tool(description = "Incremental refresh of an existing .tooned/ index: re-scans changed \
                        files and prunes deleted ones. Requires a prior tooned_index_build.")]
    async fn tooned_index_refresh(
        &self,
        Parameters(req): Parameters<IndexPathRequest>,
    ) -> Result<Json<IndexRefreshResult>, String> {
        let root = PathBuf::from(&req.path);
        let summary = tooned_index::sync(&root).map_err(|err| err.to_string())?;
        Ok(Json(IndexRefreshResult {
            files_rescanned: summary.added + summary.updated,
            files_pruned: summary.removed,
        }))
    }

    #[tool(description = "Ranked savings-opportunity report from an existing .tooned/ index, \
                        ordered by savings_pct descending.")]
    async fn tooned_stats(
        &self,
        Parameters(req): Parameters<StatsRequest>,
    ) -> Result<Json<StatsResult>, String> {
        let root = PathBuf::from(&req.path);
        let rows = tooned_index::stats(&root, req.top_n).map_err(|err| err.to_string())?;
        Ok(Json(StatsResult {
            results: rows
                .into_iter()
                .map(|row| StatsEntry { path: row.path, savings_pct: row.savings_pct })
                .collect(),
        }))
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
