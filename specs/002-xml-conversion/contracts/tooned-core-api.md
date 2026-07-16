# Contract: `tooned-core` public API (v2 XML addition)

The public surface is unchanged from v1. XML is handled by adding a `DocType::Xml` variant and a dedicated internal `xml` module. All integration surfaces (CLI, hooks, MCP server) continue to call the same three public functions.

```rust
/// Never returns Err for payload-driven failure (malformed/oversized/ambiguous
/// input) — those always resolve to Ok(Conversion::Passthrough { .. }).
/// Err is reserved for caller misuse (invalid options).
pub fn maybe_tooned(
    input: &[u8],
    opts: &ConversionOptions,
) -> Result<Conversion, ToonedError>;

/// Dry-run: same detection + shape classification as maybe_tooned, but never
/// computes or returns TOON text. Backs `tooned check`.
pub fn inspect(input: &[u8], opts: &ConversionOptions) -> InspectReport;

/// Decode a TOON document back to a structured value. Used by `tooned convert
/// --to json` and the MCP `tooned_decode` tool.
pub fn decode_toon(text: &str) -> Result<serde_json::Value, ToonedError>;

/// Supported source document types (v2 scope adds `Xml`).
pub enum DocType {
    Json,
    NdJson,
    Yaml,
    Toml,
    Csv,
    Tsv,
    Xml,
}
```

## Pre/postconditions

- **Precondition**: `input.len()` MAY exceed `opts.max_input_bytes`; `maybe_tooned` MUST check this before attempting to parse XML (FR-005) and return `Passthrough { reason: InputTooLarge }` without invoking any parser.
- **Postcondition (never-regression)**: for every `Conversion::Toon` returned, `report.toon_bytes < report.json_bytes`.
- **Postcondition (round-trip fidelity)**: for every `Conversion::Toon` returned, `decode_toon(&text)` MUST succeed and be structurally equal (after normalizing to compact JSON) to the parsed XML value.
- **Postcondition (no panics)**: `maybe_tooned` and `inspect` MUST NOT panic for any `&[u8]` input, including invalid UTF-8, truncated XML, adversarially deep nesting, malformed entities, and HTML-like `<` starts.
- **XML detection**: `detect` MUST use a path dedicated to XML (not the v1 leading-byte/line-shape sniff) and MUST NOT return `Some(DocType::Xml)` for HTML or Markdown inputs.
- **XML parsing**: `parse` for `DocType::Xml` MUST map attributes to `@`-prefixed keys, text/CDATA to `$text`/`#text` keys, repeated elements to arrays, and mixed content to ordered arrays of text and element nodes.
- **Namespace handling**: namespace prefixes MUST be stripped by default; only local element and attribute names are kept.
- **Latency**: `maybe_tooned` on a ~100 KiB well-formed XML payload of a record-list-like shape MUST complete in low single-digit milliseconds.
