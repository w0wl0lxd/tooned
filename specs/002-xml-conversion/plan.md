# Implementation Plan: XML Input Support for Adaptive TOON Conversion

**Branch**: `002-xml-conversion` | **Date**: 2026-07-15 | **Spec**: [spec.md](spec.md)  
**Input**: Feature specification from `/specs/002-xml-conversion/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Add `DocType::Xml` and a dedicated XML detection/parse path to `tooned-core` so that XML tool-call output, API responses, and config files can be adaptively re-encoded to TOON when the resulting representation is smaller than compact JSON. The XML path is intentionally separate from v1's leading-byte/line-shape sniff, uses a streaming `quick-xml` event parser, and maps XML's attribute/element/mixed-content model into `serde_json::Value` via `@`-prefixed attributes, `$text`/`#text` text nodes, and ordered arrays for mixed content. Namespace prefixes are stripped. The feature makes no public API signature changes and waits for `tooned-core` v1 to stabilize.

## Technical Context

**Language/Version**: Rust, edition 2024, resolver 3; stable toolchain is the hard CI/release gate.
**Primary Dependencies**: `quick-xml` added to `crates/tooned-core/Cargo.toml`; no new dependencies in `tooned-index` or `tooned-cli` beyond what `tooned-core` already exposes.
**Storage**: N/A (XML conversion is in-memory, like all v1 conversion).
**Testing**: `cargo nextest run --all-features` (stable-gated); `proptest` for round-trip and never-a-regression invariants extended to XML fixtures.
**Target Platform**: Linux, macOS, Windows — same as v1.
**Project Type**: CLI + library workspace (3 crates) — additive `tooned-core` feature.
**Performance Goals**: XML detection + parse should not materially slow the `maybe_tooned` hot path for non-XML inputs; XML parse should complete in a few milliseconds for typical tool-call XML payloads (~100 KiB).
**Constraints**: `tooned-core` must remain dependency-minimal and I/O-free; no external entity/DTD/network fetches; no breaking public API changes; `max_input_bytes` and structural-depth guards must apply.
**Scale/Scope**: v2 adds XML as a single new `DocType`; no new CLI subcommands or MCP tools; all existing v1 surfaces (CLI, hooks, MCP server) accept `xml` as a format hint.

## Constitution Check

The project constitution file is a placeholder, but the existing `tooned-core` code and `workspace.lints.clippy` encode the following operative principles (mirrored from 001's constitution check and the code itself):

| Principle | Check | Status |
|---|---|---|
| I. Fail-Safe Passthrough | `Conversion::Passthrough` is the default/error path; `maybe_tooned`/`inspect` never return `Err` for payload-driven failure; `quick-xml` parse errors and size/round-trip failures map to `Passthrough` | PASS (design preserves this; verified at task-level) |
| II. Measurable Savings Only | `maybe_tooned` always compares `toon_bytes` vs `json_bytes` before returning `Toon`; round-trip check required; margin default unchanged | PASS |
| III. Dependency-Minimal Core | `quick-xml` is added only to `tooned-core`; `tooned-index`/`tooned-cli` do not pull in new XML-specific deps | PASS |
| IV. Test-First | Every task in `tasks.md` follows RED→GREEN; `proptest` covers round-trip and never-a-regression invariants for XML | PASS (enforced at task-authoring time) |
| V. Complementary Scope, Not Duplication | No new CLI subcommands or MCP tools; `xml` is just a new `DocType`/`format_hint` value in existing surfaces | PASS |
| VI. Stable-Gated CI | CI already uses `cargo clippy --all-features -- -D warnings` and `cargo nextest run --all-features` on stable | PASS |

No violations requiring Complexity Tracking justification.

## Project Structure

### Documentation (this feature)

```text
specs/002-xml-conversion/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
├── tasks.md             # Phase 2 output
└── checklists/requirements.md
```

### Source Code (repository root)

```text
crates/
├── tooned-core/
│   ├── src/
│   │   ├── lib.rs            # DocType adds Xml variant
│   │   ├── detect.rs         # distinct xml::sniff branch
│   │   ├── parse.rs          # parse_xml dispatch
│   │   ├── xml.rs            # new: XML detection, parse, depth guard
│   │   ├── shape.rs          # unchanged; classifies parsed Value as usual
│   │   ├── convert.rs        # unchanged; Xml flows through existing pipeline
│   │   └── error.rs          # unchanged
│   ├── tests/
│   │   └── (existing round-trip + never-a-regression proptests add XML fixtures)
│   └── Cargo.toml            # add quick-xml dependency
├── tooned-index/             # unchanged
└── tooned-cli/               # unchanged; CLI format hint strings accept "xml"
    └── src/
        └── mcp/server.rs     # format_hint string "xml" maps to DocType::Xml
```

**Structure Decision**: The 3-crate workspace is unchanged. XML is an additive `tooned-core` feature. The only new source file is `crates/tooned-core/src/xml.rs`.

## Post-Design Constitution Re-Check

- **I. Fail-Safe Passthrough**: `xml.rs` parser returns `ParseError` on any malformed/ambiguous XML; `convert.rs` maps to `Passthrough`. No `unwrap`/`expect` on the XML hot path.
- **III. Dependency-Minimal Core**: `quick-xml` is only in `tooned-core/Cargo.toml`; CLI and index crates depend on `tooned-core` and do not need to know about XML parsing.
- **V. Complementary Scope**: No new CLI subcommands; `tooned convert/check/pipe/wrap` and MCP tools already accept a format hint string and delegate to `tooned_core`.

No new violations surfaced by Phase 1 design.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

*No violations — table intentionally omitted.*
