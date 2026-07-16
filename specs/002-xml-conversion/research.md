# Phase 0 Research: XML Input Support for Adaptive TOON Conversion

## 1. Existing `tooned-core` detection/conversion API and XML integration points

**Decision**: Add `DocType::Xml` and a new `tooned-core/src/xml.rs` module; do not change the public `maybe_tooned`, `inspect`, or `decode_toon` signatures. `detect` calls a new `xml::sniff` branch only after v1 sniffing rules fail or after a format hint is resolved, keeping the XML detection path physically distinct from v1's leading-byte/line-shape logic.

**Rationale** (from `codegraph_explore` and direct source read):
- `DocType` is an enum in `crates/tooned-core/src/lib.rs` with variants `Json`, `NdJson`, `Yaml`, `Toml`, `Csv`, `Tsv`.
- `detect.rs` uses `trim_ascii()` then looks for leading `{`/`[`, `---`, `key = value`, `key: value`, and comma/tab counts. No branch handles `<`.
- `parse.rs` dispatches on `DocType` and returns `serde_json::Value`.
- `convert.rs` (`attempt`) calls `detect -> parse -> shape::classify -> json size -> toon encode -> margin check -> round-trip check`.
- Adding a `DocType::Xml` variant and a `parse::parse_xml` branch fits the existing pipeline without touching `maybe_tooned`/`inspect`/`decode_toon` signatures.

**Constraints identified**:
- `max_input_bytes` is checked before parsing (`maybe_tooned` and `inspect`).
- `parse.rs` already has a shared structural-depth guard (`exceeds_max_structural_depth`) that only counts `{`/`[`/`}`/`]`. It is not suitable for XML element nesting (`<`/`>`/`<\/`); XML needs its own depth guard that counts `<`/`>` while ignoring brackets inside strings/CDATA.
- `toon_lsp::toon::encode` accepts `&serde_json::Value` and `decode` returns `Value`. The XML-to-JSON mapping must produce a `Value` that `toon_lsp` can encode.

## 2. XML parser library: `quick-xml`

**Decision**: Use `quick-xml` with the `serialize` feature disabled at the core conversion layer; use the streaming `Reader`/`NsReader` and event loop to map XML into `serde_json::Value` directly.

**Rationale** (from `context7` and `web_search`):
- `quick-xml` is a high-performance, pull-based, nearly zero-copy XML reader with `Cow` output and `Reader`/`NsReader` support.
- `quick-xml` supports `Event` variants: `Start`, `End`, `Empty`, `Text`, `CData`, `Comment`, `Decl`, `PI`, `DocType`, `GeneralRef`, `Eof`.
- `quick-xml` has a `serde` deserialization module, but it requires a known schema and uses `@`/`$text`/`$value` conventions. It is not suitable for schema-less conversion of arbitrary XML to `serde_json::Value` because `serde_json::Value` is too generic for quick-xml's deserializer to produce a predictable shape.
- A custom event-loop parser gives full control over attribute/element/mixed-content mapping and lets us bail out to `ParseFailed` on any ambiguous construct.

**Alternatives considered**:
- `serde-xml-rs` — higher-level, but similarly schema-driven and not maintained as actively as `quick-xml`.
- `xml-rs` — pure Rust, but slower and not zero-copy; rejected because `quick-xml` is the de facto standard for this use case.
- `roxmltree` — DOM-style, loads whole document; rejected to avoid large-document allocation and to match the streaming/low-allocation spirit of `tooned-core`.

## 3. XML-to-JSON mapping convention

**Decision**: Adopt a BadgerFish-style mapping with `quick-xml`'s `@`/`$text`/`$value` conventions, tuned for LLM context and TOON's array-of-objects sweet spot:

| XML construct | JSON representation | Notes |
|---|---|---|
| Element with attributes and no children | `{"@attr": "value", "$text": "text"}` | Attributes are `@`-prefixed; `$text` holds text/CDATA. |
| Element with attributes and child elements | `{"@attr": "value", "child": [...]}` | No `$text` if all children are elements. |
| Element with only text | `"text"` or `{"$text": "text"}` if attributes present. |
| Repeated child elements | array of values | e.g. `<item/><item/>` → `"item": [{}, {}]`. |
| Mixed content | array of text nodes (`{"#text": "text"}`) and element nodes | Preserves order. |
| Namespace prefixes | stripped | only local name kept. |
| XML declaration/DOCTYPE/Comment/PI | ignored | not represented in JSON. |

**Rationale**:
- `quick-xml` documentation and `serde` conventions use `@` for attributes and `$text`/`$value` for text/mixed content. Using `@` and `$text` makes the output familiar and predictable.
- Mapping mixed content to an ordered array preserves XML semantics but often produces JSON that is not smaller than the original XML, so it naturally falls through the size gate for document-oriented XML.
- Stripping namespace prefixes keeps keys short and avoids extremely verbose namespace URIs in the LLM context.

## 4. XML detection

**Decision**: Implement a dedicated `xml::sniff(input: &[u8]) -> Option<DocType::Xml>` in `tooned-core/src/detect.rs` (or in a new `xml.rs` module called by `detect`):
- Skip leading UTF-8 BOM and ASCII whitespace.
- If the next bytes are `<?xml`, `<?XML`, `<!DOCTYPE`, or `<!doctype`, treat as XML.
- Otherwise, if the next byte is `<` and the following bytes match a valid XML element-start name (e.g. `<root ...` or `<root/>`), treat as XML.
- Explicitly reject HTML markers: `<!DOCTYPE html`, `<!DOCTYPE HTML`, `<html`, `<head`, `<body`, `<div`, `<span`, `<p`, `<a `, `<script`, `<style`, `<table`, etc.
- Reject inputs where the first `<` is not followed by a valid XML name start character (`_`, `:`, or letter) or `?`/`!`.

**Rationale**:
- The feature description explicitly requires a detection path distinct from v1's leading-byte/line-shape sniff.
- HTML false-positives are the main risk with naive `<` detection. HTML tag names are a known set; rejecting them by name is a conservative, fast heuristic.
- XML detection is intentionally conservative: if the input is ambiguous, it returns `None`, and the v1 pipeline or passthrough behavior continues unchanged.

## 5. XML viability and the TOON sweet spot

**Decision**: The conversion decision remains purely size-based (as in v1). The XML parser does not pre-reject mixed-content or namespace-heavy documents; it produces a `Value` and lets `convert.rs` decide whether TOON is smaller. The `InspectReport` will include the `ShapeClass` (which for XML is still computed on the top-level `Value`), so the `tooned check` output can explain why a payload did or did not convert.

**Rationale** (from `thoughtbox` structured reasoning):
- TOON's array-of-objects sweet spot corresponds to XML that is "record-list-like": a root element whose children are repeated, similarly-shaped elements with attributes or simple child elements.
- Attribute-heavy XML is especially promising because `toon_lsp` can drop the repeated attribute names in favor of a positional/typed record layout.
- Mixed-content documents usually produce JSON that is larger than the original XML because of `#text`/`$value` wrappers and text-node arrays; they will fail the size gate and pass through.
- Namespace-heavy SOAP envelopes produce verbose JSON keys; the size gate and round-trip check will naturally reject them.

## 6. XML parsing safety

**Decision**: Guard XML parsing the same way JSON is guarded:
- `max_input_bytes` check before parsing (already in `maybe_tooned`/`inspect`).
- A new XML-specific structural depth guard that counts `<`/`>` nesting while ignoring `<!--` comments, `<![CDATA[...]]>` sections, and quoted attribute strings.
- No external entity/DTD fetching; `quick-xml` does not fetch DTDs by default unless configured.
- Treat invalid UTF-8 as `ParseFailed` (XML is required to be Unicode/UTF-8 unless an encoding is declared; we only support UTF-8 in v2).
- Treat malformed XML (unclosed tags, duplicate attributes, unbalanced elements) as `ParseFailed`.

**Rationale**:
- Constitution Principle I (Fail-Safe Passthrough) and FR-006 require no crashes on adversarial input.
- XML billion-laughs/entity expansion attacks are mitigated by not resolving external entities and by the `max_input_bytes` cap.
- A depth guard prevents stack overflow on adversarially nested `<a><a><a>...</a></a></a>`.

## 7. arXiv and web research

- **arXiv** search for XML serialization/parsing/tokenization in LLM contexts returned no directly relevant papers; the field is dominated by XML Schema/ontology work rather than XML-to-token-size optimization. The size decision is therefore driven by empirical byte comparison rather than a published XML-tokenization model.
- **Web search** confirmed `quick-xml` is the standard Rust XML parser and documented the `@`/`$text`/`$value` conventions and `NsReader` namespace resolution. The `quickxml_to_serde` crate was identified as a prior art example but it uses quick-xml's `serde` deserialization and is schema-agnostic only at the top level; it is not as controllable as a custom event loop.
- **Exa** search failed due to missing `EXA_API_KEY`; the requirement to fall back to built-in `web_search` was satisfied.

## 8. Summary of open decisions settled

- Use `quick-xml` streaming reader with a custom event loop, not `serde` deserialization.
- Add `DocType::Xml`; no public API signature changes.
- Use `@` prefix for attributes and `$text`/`#text` for text; mixed content becomes an ordered array.
- Strip namespace prefixes; keep local names.
- XML detection is a separate, conservative heuristic that rejects HTML tags.
- Conversion viability is decided by the existing size/round-trip gate, not by a separate XML pre-filter.
