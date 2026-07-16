# 002 XML conversion implementation

- **Date:** 2026-07-16
- **Author:** w0wl0lxd
- **Branch:** `002-xml-conversion`
- **PR(s):** [#4](https://github.com/w0wl0lxd/tooned/pull/4)

## Context

Feature 002 (`xml-conversion`) was planned as a v2 addition to add XML input support to the adaptive TOON conversion pipeline. The spec-kit (T001–T012) had been completed on 2026-07-15, defining the requirements for XML detection, parsing, and conversion. The implementation phase (T013–T043) was executed on 2026-07-16, followed by an independent review that found two correctness issues (entity reference resolution and `xml:*` attribute preservation).

## Reasoning

XML required a distinct detection path from the v1 doctypes (JSON/NDJSON/YAML/TOML/CSV/TSV) because the leading-byte/line-shape sniff used for those formats would false-positive on HTML/Markdown or miss XML entirely. The design chose:

- **`DocType::Xml`**: A new enum variant in the existing `tooned-core` API, avoiding any breaking changes to `maybe_tooned`, `inspect`, or `decode_toon`.
- **`quick-xml`**: A streaming event-based parser with minimal dependencies, chosen over `serde_xml_rs` (which pulls in `log` and has more complex error handling) and `roxmltree` (which builds a full tree in memory). `quick-xml` allows depth-guarded parsing, namespace stripping, and mixed-content handling without loading the entire document.
- **Conservative detection**: The XML sniffer explicitly rejects HTML `<!DOCTYPE html>` and common HTML tags (`<html>`, `<head>`, `<body>`, `<div>`, `<span>`, `<p>`, `<a>`, `<img>`, `<script>`, `<style>`) to avoid false positives on web content.

## Steps taken

1. **Spec-kit (T001–T012)**: Completed on 2026-07-15, defining XML detection, parsing, and conversion requirements.
2. **Detection (T013–T018)**: Implemented XML-specific sniff in `detect.rs` that checks for XML declarations, `DOCTYPE` declarations, and element starts while rejecting HTML/Markdown.
3. **Parsing (T019–T028)**: Implemented `xml.rs` module with `quick-xml` event-based parsing:
   - Streaming events with depth guard (max 256 levels).
   - Namespace prefix stripping (local names only).
   - Attribute-to-`@`-prefixed key mapping.
   - Mixed-content handling (text nodes as `#text`).
   - Repeated child elements as arrays.
4. **Integration (T029–T036)**: Wired `DocType::Xml` into the existing `maybe_tooned`/`inspect` pipeline, added CLI `--format-hint xml` support, and MCP `format_hint: "xml"` support.
5. **Testing (T037–T042)**: Added XML-specific property tests:
   - `xml_roundtrip_proptest.rs`: XML → JSON → XML round-trip fidelity.
   - `xml_never_regression_proptest.rs`: Never-a-regression across adversarial XML.
   - `xml_no_panic_proptest.rs`: No-panic on invalid UTF-8, truncated, HTML-like, or deeply-nested XML.
   - CLI/MCP format-hint coverage tests for all doctypes including XML.
6. **Independent review**: An adversarial + security-review subagent identified two correctness issues:
   - Entity references (`&#65;`, `&#x41;`, `&lt;`, `&amp;`, etc.) were not being resolved in text content.
   - `xml:*` attributes (e.g., `xml:lang`, `xml:space`) were being stripped along with namespaces.
7. **Fixes (T043)**:
   - Added entity reference resolution for character refs and predefined entities; unknown custom entities remain literal `&name;`.
   - Preserved `xml:*` attributes in parsed output; custom entity references preserved as literal text.
8. **Verification**: Re-ran the full release gate (`fmt`, `clippy`, `nextest`, `machete`, `audit`, `deny`) and committed/pushed the fixes.

## Verification

```bash
cd /home/w0w/dev/tooned
export RUSTC_WRAPPER="" && export CARGO_TARGET_DIR=/tmp/cargo-target-tooned
cargo fmt --all
cargo clippy --workspace --all-features -- -D warnings
cargo nextest run --workspace --all-features --no-fail-fast
cargo machete --with-metadata
cargo audit
cargo deny check
```

Observed output:
- `cargo fmt --all` — PASS (exit 0)
- `cargo clippy --workspace --all-features -- -D warnings` — PASS (exit 0)
- `cargo nextest run --workspace --all-features --no-fail-fast` — `226 tests run: 226 passed, 1 skipped`
- `cargo machete --with-metadata` — PASS (no unused dependencies)
- `cargo audit` — PASS (no vulnerabilities)
- `cargo deny check` — `advisories ok, bans ok, licenses ok, sources ok`

## PR description

Implements XML input support for adaptive TOON conversion, including conservative detection, streaming `quick-xml` parsing with depth guards, namespace stripping, mixed-content handling, and entity reference resolution. Adds XML-specific property tests and CLI/MCP format-hint coverage. Fixes independent review findings for entity refs and `xml:*` attribute preservation.

## Follow-ups

- Monitor PR checks and CodeRabbit comments; address any new review findings.
- The XML parser does not resolve external entities or DTDs; any input requiring external resolution is passed through (by design per spec).
- Mixed-content XML documents (e.g., XHTML, DocBook) are expected to pass through rather than convert, as they do not fit TOON's array-of-objects sweet spot.

## Changelog

Inserted under `### Fixed` (line **86**) in [CHANGELOG.md](../../../CHANGELOG.md), immediately AFTER the preceding bullet that begins with **Dual licensing (AGPL-3.0-only + commercial).** The CHANGELOG bullets for the XML follow-up fixes record the reverse reference via the Markdown link `[work-log](docs/agents/work-log/2026-07-16-002-xml-conversion.md)`.
