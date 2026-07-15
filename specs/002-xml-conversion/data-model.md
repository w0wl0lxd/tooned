# Data Model: XML Input Support for Adaptive TOON Conversion

Derived from spec.md's Key Entities section, made concrete for the Rust workspace.

## `DocType` (`tooned-core`)

```rust
pub enum DocType {
    Json,
    NdJson,
    Yaml,
    Toml,
    Csv,
    Tsv,
    Xml, // v2
}
```

**Rules**:
- `DocType::Xml` is a first-class variant, returned by `detect.rs` and consumed by `parse.rs`.
- `ConversionOptions`, `ConversionReport`, `InspectReport`, and `PassthroughReason` all use `DocType` unchanged; they are already generic over the `DocType` enum.

## XML Detection (`tooned-core`)

```rust
// new module: crates/tooned-core/src/xml.rs
pub fn sniff(input: &[u8]) -> Option<DocType>;
```

**Rules**:
- Skip leading UTF-8 BOM and ASCII whitespace.
- Recognize XML declaration (`<?xml ...?>`), `DOCTYPE` (`<!DOCTYPE ...>`), and element start (`<name ...>` or `<name/>`).
- Reject HTML tag starts (`<!DOCTYPE html`, `<html`, `<head`, `<body`, `<div`, `<span`, `<p`, `<a `, `<script`, `<style`, `<table`, etc.).
- Reject plain text whose first `<` is not followed by a valid XML start (`?`, `!`, or name-start char).
- Detection is conservative: when in doubt, return `None`.

## XML Parse (`tooned-core`)

```rust
// new module: crates/tooned-core/src/xml.rs
pub fn parse(input: &[u8]) -> Result<serde_json::Value, ParseError>;

pub struct XmlParseOptions {
    // v2 defaults; no public API yet
    pub max_depth: usize,          // default 100
    pub strip_namespaces: bool,    // default true
    pub mixed_content_key: String, // default "#text"
    pub attribute_prefix: String,  // default "@"
    pub text_key: String,          // default "$text"
}
```

**Rules**:
- Input must be valid UTF-8; otherwise `ParseError::Utf8`.
- Input nesting must not exceed `max_depth`; otherwise `ParseError::TooDeep`.
- No external entities or DTDs are fetched.
- The parser produces `serde_json::Value` from `quick-xml` events.
- Attribute values are unescaped and decoded as strings.
- Text/CDATA content is concatenated and decoded.
- If `strip_namespaces` is true, only local names are used for element and attribute keys.
- Mixed-content elements are represented as an array of text nodes (`{"#text": "..."}`) and element nodes (`{"tag": {...}}` or `{"tag": "..."}`), preserving order.
- Repeated child elements become JSON arrays under the same key.
- Empty elements are represented as `{}` unless they have attributes, in which case they are represented as `{"@attr": "value"}`.

## XML-to-JSON Mapping Examples

```xml
<?xml version="1.0"?>
<library>
  <book id="1" lang="en">
    <title>XML in Rust</title>
    <author>Alice</author>
  </book>
  <book id="2" lang="fr">
    <title>XML en Rust</title>
    <author>Bob</author>
  </book>
</library>
```

→

```json
{
  "library": {
    "book": [
      {
        "@id": "1",
        "@lang": "en",
        "title": "XML in Rust",
        "author": "Alice"
      },
      {
        "@id": "2",
        "@lang": "fr",
        "title": "XML en Rust",
        "author": "Bob"
      }
    ]
  }
}
```

Mixed-content example:

```xml
<p>Hello <b>world</b>!</p>
```

→

```json
{
  "p": [
    {"#text": "Hello "},
    {"b": "world"},
    {"#text": "!"}
  ]
}
```

## `ConversionReport` and `InspectReport` (`tooned-core`)

No changes. `doc_type` will be `DocType::Xml` when XML is detected and parsed.

```rust
pub struct ConversionReport {
    pub doc_type: DocType, // Xml when XML conversion succeeds
    pub shape: ShapeClass,
    pub json_bytes: usize,
    pub toon_bytes: usize,
    pub savings_pct: f64,
}

pub struct InspectReport {
    pub doc_type: Option<DocType>,
    pub shape: ShapeClass,
    pub input_bytes: usize,
    pub json_bytes: Option<usize>,
    pub toon_bytes: Option<usize>,
    pub savings_pct: Option<f64>,
    pub precise_savings_pct: Option<f64>,
    pub would_convert: bool,
    pub reason: Option<PassthroughReason>,
}
```

## `PassthroughReason` (`tooned-core`)

No changes. The same variants apply to XML:

- `NotStructuredData` — XML detector returned `None`.
- `ParseFailed` — XML detector said `Xml` but `parse_xml` failed.
- `InputTooLarge` — `input.len() > max_input_bytes`.
- `NotSmallerEnough { json_bytes, toon_bytes }` — parsed XML but TOON is not smaller by margin.
- `RoundTripMismatch` — TOON decode did not match the parsed value.

## `ConversionOptions` (`tooned-core`)

No changes. `format_hint: Option<DocType>` can be `Some(DocType::Xml)`. `max_input_bytes` and `margin_pct` apply unchanged.

## API Surface Changes

```rust
// crates/tooned-core/src/lib.rs
pub enum DocType {
    // ... existing variants ...
    Xml,
}

// public functions are unchanged
pub fn maybe_tooned(input: &[u8], opts: &ConversionOptions) -> Result<Conversion, ToonedError>;
pub fn inspect(input: &[u8], opts: &ConversionOptions) -> InspectReport;
pub fn decode_toon(text: &str) -> Result<serde_json::Value, ToonedError>;
```

**Rules**:
- `maybe_tooned` and `inspect` continue to call `detect` then `parse`; adding `Xml` is the only change.
- `decode_toon` is not XML-aware; it decodes TOON text to `Value`, which is the same for all doctypes.

## CLI / MCP Tool Input Strings

`tooned-cli` and `tooned-cli/src/mcp/server.rs` accept lowercase `xml` (and optionally `XML`) as a format hint string and map it to `DocType::Xml`. No new commands or tools.
