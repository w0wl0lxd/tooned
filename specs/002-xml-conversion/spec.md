# Feature Specification: XML Input Support for Adaptive TOON Conversion

**Feature Branch**: `002-xml-conversion`
**Created**: 2026-07-15
**Status**: Implemented (2026-07-16)  
**Input**: User description: "v2 support for XML input format. XML needs its own detection path (distinct from the leading-byte/line-shape sniff used for v1 doctypes) and its own viability analysis for adaptive TOON conversion (attribute vs. element modeling, mixed content, namespaces don't map cleanly onto TOON's array-of-objects sweet spot). This should wait until the core detection/conversion API in `tooned-core` stabilizes so XML support doesn't force an early API redesign."  
**Issue**: GitHub issue #1  

## Clarifications

### Session 2026-07-15

- Q: Should XML support be a separate `DocType::Xml` in the existing `tooned-core` API, or a wholly new module? → A: A new `DocType::Xml` variant plus a dedicated `xml` module in `tooned-core`; no public function signature changes (`maybe_tooned`, `inspect`, `decode_toon` stay the same).
- Q: Should the XML conversion attempt to preserve namespaces in the JSONified output? → A: By default, namespace prefixes are stripped and only local element/attribute names are used; full namespace URI preservation is out of scope for v2 (too verbose for LLM contexts).
- Q: Does the v2 XML feature require CLI changes beyond accepting `xml` as a `--format`/`format_hint` value? → A: No new subcommands; existing `tooned convert`, `tooned check`, `tooned pipe`, `tooned wrap`, and MCP tools accept `xml` as a format hint once the core `DocType` enum includes `Xml`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Convert XML API Responses and Config Files (Priority: P1)

A developer receives XML-shaped tool output (for example, a legacy SOAP/REST response, an XML config file, or an RSS/Atom feed) and wants it to be adaptively re-encoded to TOON whenever doing so reduces size versus compact JSON. The conversion must not require the developer to declare the format, and if the XML is not a good fit for TOON, the original XML is passed through unchanged.

**Why this priority**: XML remains common in enterprise APIs (SOAP, OData, sitemaps, RSS/Atom, Maven/Gradle XML, Android manifests, plist fragments), and agent sessions frequently consume these. This is the core conversion value of the feature.

**Independent Test**: Can be fully tested by passing representative XML payloads (attribute-heavy records, repeated child elements) through `tooned pipe` or `tooned check` and confirming conversion only when the resulting JSON/TOON is smaller and semantically faithful.

**Acceptance Scenarios**:

1. **Given** an XML document whose root contains a list of uniformly-shaped records with attributes and child elements, **When** `maybe_tooned` processes it, **Then** it returns `Conversion::Toon` only if the TOON representation is smaller than the equivalent compact JSON by more than the configured margin.
2. **Given** an XML document with mixed content (interleaved text and elements), deeply recursive tags, or a document-oriented shape, **When** `maybe_tooned` processes it, **Then** it returns `Conversion::Passthrough` with `NotStructuredData` or `ParseFailed` or `NotSmallerEnough` as appropriate.
3. **Given** an XML payload that is malformed, truncated, or exceeds `max_input_bytes`, **When** `maybe_tooned` processes it, **Then** it returns the original XML unchanged via `Conversion::Passthrough`.

---

### User Story 2 - Standalone XML Detection and Inspection (Priority: P2)

A developer wants to know whether an XML file or stream is a good candidate for TOON conversion before committing to conversion — for example, to decide whether an XML config dump is worth re-encoding.

**Why this priority**: Builds trust and debuggability; mirrors the `tooned check` workflow already expected from v1. It is the foundation of the v1 CLI/MCP integration for XML.

**Independent Test**: Can be fully tested by running `tooned check some-file.xml` or the MCP `tooned_detect` tool with XML content and confirming the report includes `doc_type: Xml`, a shape class, and a would_convert verdict.

**Acceptance Scenarios**:

1. **Given** an XML file on disk, **When** the developer runs `tooned check` on it, **Then** the report shows `Xml` as the detected doctype, the shape classification, and the byte-size comparison.
2. **Given** an XML file with a `.xml` extension but content that is not actually XML, **When** `tooned check` is run, **Then** the report reflects content-based detection, honoring an explicit `--format xml` hint if supplied.

---

### User Story 3 - Safe, Non-Breaking Integration with Existing v1 Surfaces (Priority: P2)

A developer already has tooned v1 installed and uses the CLI, Claude Code hook, Codex CLI hook, or MCP server. Enabling XML support must not break existing behavior for JSON/NDJSON/YAML/TOML/CSV/TSV payloads.

**Why this priority**: XML is an additive v2 feature; it must not destabilize the v1 surfaces that are already in production. It must wait for the core API to stabilize so no breaking signature changes are required.

**Independent Test**: Can be fully tested by re-running the v1 contract tests against the branch and confirming all pass unchanged, then adding XML-only fixtures.

**Acceptance Scenarios**:

1. **Given** a v1 JSON/YAML/TOML/CSV/TSV payload, **When** processed by the updated `tooned-core`, **Then** the behavior is byte-for-byte identical to the v1 output.
2. **Given** an MCP `tooned_convert` call with `"format_hint": "xml"`, **When** the content is valid XML, **Then** the tool returns the same adaptive `Conversion` result shape as for other doctypes.

---

### Edge Cases

- What happens when an XML document has a `<!DOCTYPE>` declaration with external entity references? → The parser does not fetch external entities; such input is treated as parse-failed and passed through.
- What happens when an XML element has the same name as an attribute? → The attribute is prefixed with `@` in the JSONified representation, so `id` (attribute) and `id` (element) do not collide.
- What happens when XML contains repeated child elements of the same name? → They are represented as an array of values under that key, preserving order.
- What happens when an element has mixed content (text and child elements interleaved)? → The element is represented as an array of mixed text-nodes (key `#text`) and element-nodes, preserving order; if the resulting JSON is not smaller, it passes through.
- What happens when XML uses namespaces? → By default, namespace prefixes are stripped; the local name is used. Namespace URIs are not preserved in the JSONified output.
- What happens when XML starts with a BOM or whitespace before the XML declaration? → Detection strips leading whitespace/BOM and still recognizes the XML declaration or root element.
- What happens when XML is detected as XML but the leading bytes are ambiguous (e.g., an HTML `<!DOCTYPE html>` document)? → HTML is explicitly rejected by the XML detector; HTML-like `<` starts are treated as unrecognized or passed through.
- What happens when the XML root element contains a single text value with no attributes and no children? → It is represented as a JSON string or a scalar object, depending on whether attributes are present; if the result is not smaller, it passes through.
- What happens when an XML comment, processing instruction, or `DOCTYPE` contains malformed content? → Non-structural content is skipped where possible; malformed declarations that prevent parsing result in `ParseFailed` passthrough.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST detect XML input via a dedicated detection path that is separate from v1's leading-byte/line-shape sniff. The detection path MUST recognize XML declarations (`<?xml ...?>`), `DOCTYPE` declarations, and XML element starts, and MUST NOT false-positive on HTML, Markdown, or plain text that happens to contain `<` characters.
- **FR-002**: System MUST add `Xml` as a supported `DocType` variant in `tooned-core` without changing the public `maybe_tooned`, `inspect`, or `decode_toon` signatures.
- **FR-003**: System MUST parse valid XML into a `serde_json::Value` representation that maps attributes to `@`-prefixed keys, child elements to nested objects/arrays, and text content to a `#text` or `$text` key, preserving order for mixed content.
- **FR-004**: System MUST compute the compact-JSON size of the parsed XML value and the TOON size, and return TOON only when it is smaller by more than the configured `margin_pct`.
- **FR-005**: System MUST fall back to passing the original XML through whenever parsing fails, the input exceeds `max_input_bytes`, the input is malformed or ambiguous, the parsed XML would not shrink under TOON, or the round-trip check fails.
- **FR-006**: System MUST NOT crash, hang, or block on any XML input, including malformed, truncated, adversarially deep, invalid-UTF-8, or entity-expansion payloads.
- **FR-007**: System MUST verify round-trip fidelity: the parsed value encoded to TOON and decoded back must be structurally equal (after normalizing to compact JSON); otherwise it MUST pass through.
- **FR-008**: System MUST honor an explicit `format_hint = Xml` even if it conflicts with the actual content, with the same fail-safe passthrough behavior as other doctypes.
- **FR-009**: System MUST support XML in all existing v1 integration surfaces (CLI `convert`/`check`/`pipe`/`wrap`, Claude Code hook, Codex CLI hook, MCP `tooned_convert`/`tooned_detect`/`tooned_decode`) without new subcommands or tool names.
- **FR-010**: System MUST provide a `tooned check` report for XML that includes `doc_type: Xml`, `shape`, `json_bytes`, `toon_bytes`, `savings_pct`, and `would_convert`.
- **FR-011**: System MUST NOT fetch external entities or DTDs, and MUST NOT perform network calls during XML parsing.
- **FR-012**: System MUST NOT mutate any source file; all XML conversion happens on data in transit.
- **FR-013**: System MUST document which XML shapes are expected to convert well (attribute-heavy, record-list-like) and which are expected to pass through (mixed-content, document-oriented, namespace-heavy SOAP envelopes).

### Key Entities

- **XML Detection Result**: The outcome of the XML-specific sniff, distinct from v1 detection, including whether the input is confidently XML and whether it looks like HTML/Markdown.
- **XML Viability Profile**: A classification of the parsed XML shape (record-list-like, attribute-heavy, mixed-content, document-oriented) used to explain `would_convert` but not to gate the conversion decision.
- **XML-to-JSON Mapping**: The rule set that maps XML elements, attributes, text nodes, repeated elements, and namespaces into `serde_json::Value`.
- **Conversion Decision**: Reused from v1; for XML, `doc_type` is `Xml` and the same size/round-trip rules apply.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: XML payloads that are attribute-heavy or record-list-like and chosen for conversion are reduced in size versus compact JSON in 100% of cases where a `Conversion::Toon` is applied.
- **SC-002**: v1 payloads (JSON/NDJSON/YAML/TOML/CSV/TSV) processed by the updated `tooned-core` produce byte-identical output and identical `ConversionReport` values to the v1 implementation, verified by the existing v1 contract tests.
- **SC-003**: Malformed, oversized, ambiguous, or HTML-like XML input reaches the caller unchanged in 100% of observed cases.
- **SC-004**: A developer can run `tooned check file.xml` and receive a correct `doc_type`, `shape`, and `would_convert` verdict without needing to declare the format.
- **SC-005**: XML support does not require any breaking change to the `tooned-core` public API (`maybe_tooned`, `inspect`, `decode_toon`) or to the CLI/MCP tool names.

## Assumptions

- `tooned-core` v1 is stable; the v2 XML feature will not force a public API redesign.
- XML namespace URIs are not needed in the LLM context target; stripping prefixes and using local names is acceptable.
- Mixed-content XML documents (e.g., XHTML, DocBook, text-heavy RSS descriptions) are expected to pass through rather than convert, because they do not fit TOON's array-of-objects sweet spot.
- The XML parser will be a pull/event-based parser (`quick-xml`) to avoid loading large documents into memory and to support streaming-style detection.
- XML external entities are not resolved; any input that requires external DTD resolution is not supported and will be passed through.
- XML detection is intentionally conservative: if there is any doubt whether `<` indicates XML or HTML/Markdown, the input is passed through.
