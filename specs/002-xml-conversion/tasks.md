---

description: "Task list for feature 002-xml-conversion"
---

# Tasks: XML Input Support for Adaptive TOON Conversion

**Input**: Design documents from `/specs/002-xml-conversion/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Included and REQUIRED — constitution Principle IV (Test-First, NON-NEGOTIABLE) mandates RED→GREEN TDD for every task, with `proptest` coverage for the two safety invariants (round-trip fidelity, never-a-regression).

**Organization**: Tasks are grouped by user story (P1–P2 from spec.md) so each can be implemented, tested, and delivered independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1 (P1, XML conversion), US2 (P2, XML detection/inspection), US3 (P2, safe integration)
- File paths are exact and repo-relative to `/home/w0w/dev/tooned`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add the new XML dependency and module scaffolding to the existing `tooned-core` crate.

- [x] T001 Add `quick-xml = "0.41.0"` to `crates/tooned-core/Cargo.toml` with no extra features enabled (keep `tooned-core` dependency-minimal). Run `cargo deny check` to confirm no new bans/license violations.
- [x] T002 [P] Add `mod xml;` and `pub use xml::XmlParseOptions;` (optional) in `crates/tooned-core/src/lib.rs`; add `DocType::Xml` to the `DocType` enum.
- [x] T003 [P] Create `crates/tooned-core/src/xml.rs` with empty `pub fn sniff` and `pub fn parse` stubs and a `pub struct XmlParseOptions` skeleton.
- [x] T004 [P] Update `crates/tooned-cli/src/mcp/server.rs` and `crates/tooned-cli/src/cli/mod.rs` to recognize `"xml"` (and `"XML"`) as a valid `format_hint` string mapping to `DocType::Xml`.
- [x] T005 Run `cargo check --lib` and `cargo clippy --all-features --all-targets -- -D warnings` against the scaffold; fix any warnings before proceeding.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: `tooned-core`'s XML detection and parse pipeline is fully implemented and tested. Nothing in Phase 3+ can start until this is GREEN.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Tests for Foundational Phase (write FIRST, confirm RED)

- [x] T006 [P] Unit tests for XML detection in `crates/tooned-core/src/xml.rs`: valid XML declaration, `DOCTYPE`, simple root element, repeated element; HTML rejection (`<!DOCTYPE html`, `<html`, `<div`, `<script`, `<table`); Markdown `<` false positives; plain text with `<` false positives; BOM/whitespace tolerance.
- [x] T007 [P] Unit tests for XML parse in `crates/tooned-core/src/xml.rs`: attributes to `@`-prefixed keys, repeated child elements to arrays, text-only element to string, element with attributes + text to `{"@attr": "...", "$text": "..."}`, mixed content to ordered array of `{"#text": ...}` and `{"tag": ...}` nodes, CDATA as text, comments/PIs ignored, namespace prefixes stripped.
- [x] T008 [P] Unit tests for XML structural depth guard in `crates/tooned-core/src/xml.rs`: adversarially nested `<a><a>...</a></a>` past `max_depth` returns `ParseError::TooDeep`; brackets inside CDATA/quoted strings do not count.
- [x] T009 [P] Unit tests for XML format hint override in `crates/tooned-core/src/detect.rs`: `detect(b"not xml", Some(DocType::Xml))` returns `Some(DocType::Xml)`; `detect(valid_xml, Some(DocType::Json))` returns `Some(DocType::Json)`.
- [x] T010 [P] Unit tests in `crates/tooned-core/src/convert.rs`: XML input exceeding `opts.max_input_bytes` returns `Passthrough { reason: InputTooLarge }` without invoking the XML parser.
- [x] T011 [P] Unit tests in `crates/tooned-core/src/convert.rs`: XML payloads that are smaller as TOON convert; XML payloads that are not smaller by `margin_pct` return `Passthrough { reason: NotSmallerEnough }`.
- [x] T012 [P] Property test: for every XML input where `maybe_tooned` returns `Conversion::Toon`, `decode_toon(&text)` succeeds and is structurally equal to the parsed value — `crates/tooned-core/tests/xml_roundtrip_proptest.rs`.
- [x] T013 [P] Property test: for every XML input where `maybe_tooned` returns `Conversion::Toon`, `report.toon_bytes < report.json_bytes` — `crates/tooned-core/tests/xml_never_regression_proptest.rs`.
- [x] T014 [P] Property test: `maybe_tooned` and `inspect` never panic for any XML or non-XML `&[u8]` input, including invalid UTF-8, HTML, and truncated XML — `crates/tooned-core/tests/xml_no_panic_proptest.rs`.

### Implementation for Foundational Phase

- [x] T015 Implement `xml.rs::XmlParseOptions` with defaults (`max_depth = 100`, `strip_namespaces = true`, `mixed_content_key = "#text"`, `attribute_prefix = "@"`, `text_key = "$text"`).
- [x] T016 Implement `xml.rs::sniff`: skip BOM/whitespace, recognize `<?xml`, `<!DOCTYPE`, and valid XML element starts, reject HTML tag names, return `Option<DocType>` (GREEN T006).
- [x] T017 Implement `xml.rs::parse`: streaming `quick-xml::Reader` event loop, depth guard, `@`-prefixed attributes, `$text`/`#text` text nodes, repeated elements to arrays, mixed content to ordered arrays, namespace stripping, CDATA/comment/PI handling, returns `Result<Value, ParseError>` (GREEN T007, T008).
- [x] T018 Update `detect.rs::sniff` to call `xml::sniff` as a separate, late branch after the v1 sniffs, so XML detection does not interfere with JSON/NDJSON/YAML/TOML/CSV/TSV detection (GREEN T006, T009).
- [x] T019 Update `parse.rs::parse` to dispatch `DocType::Xml` to `xml::parse` and add a `ParseError::Xml` variant (GREEN T007).
- [x] T020 Update `convert.rs::attempt` to handle `DocType::Xml` through the existing size/round-trip pipeline (no logic changes; Xml flows through the existing path) (GREEN T010, T011).
- [x] T021 Audit `xml.rs` for `unwrap`/`expect`/`panic!`/indexing that could panic on adversarial XML; replace with `ParseError` (GREEN T014).
- [x] T022 Run `cargo clippy --all-features --all-targets -- -D warnings` and `cargo fmt --all -- --check` on `tooned-core`; confirm T006–T014 all GREEN.

**Checkpoint**: `tooned-core`'s XML detection and parse pipeline is fully implemented and tested. All user stories below may now proceed.

---

## Phase 3: User Story 1 - Convert XML API Responses and Config Files (Priority: P1) 🎯 MVP

**Goal**: XML tool-call output and files are transparently converted when TOON is smaller, with fail-safe passthrough on any doubt.

**Independent Test**: Pass representative XML through `tooned pipe`/`tooned check` and confirm conversion or passthrough behavior; confirm `tooned-core` public API returns `Conversion::Toon` only when smaller.

### Tests for User Story 1 (write FIRST, confirm RED)

- [x] T023 [P] [US1] Integration test: `maybe_tooned` on an attribute-heavy XML record list returns `Conversion::Toon` with `doc_type == DocType::Xml` and `savings_pct > 0` — `crates/tooned-core/tests/xml_convert.rs`.
- [x] T024 [P] [US1] Integration test: `maybe_tooned` on mixed-content XML or a SOAP-like envelope returns `Conversion::Passthrough` (not converted) — `crates/tooned-core/tests/xml_convert.rs`.
- [x] T025 [P] [US1] Integration test: `tooned pipe` with XML stdin prints converted output when smaller, original XML when not — `crates/tooned-cli/tests/cli_pipe_xml.rs`.

### Implementation for User Story 1

- [x] T026 [US1] Wire `tooned pipe` to accept `--format xml` and delegate to `maybe_tooned` (no new logic; CLI already delegates) (GREEN T025).
- [x] T027 [US1] Run `cargo clippy --all-features --all-targets -- -D warnings` and `cargo fmt --all -- --check` on the CLI pipe module; confirm T023–T025 all GREEN.

**Checkpoint**: User Story 1 is independently functional — XML conversion works in the standalone CLI.

---

## Phase 4: User Story 2 - Standalone XML Detection and Inspection (Priority: P2)

**Goal**: `tooned check` and the MCP `tooned_detect` tool correctly report XML doctype, shape, and conversion viability.

**Independent Test**: Run `tooned check some-file.xml` and `tooned_detect` with XML content and confirm the report includes `doc_type: Xml` and `would_convert`.

### Tests for User Story 2 (write FIRST, confirm RED)

- [x] T028 [P] [US2] Contract test: `tooned check file.xml` prints `doc_type: Xml`, `shape`, `json_bytes`, `toon_bytes`, `savings_pct`, and `would_convert` — `crates/tooned-cli/tests/cli_check_xml.rs`.
- [x] T029 [P] [US2] Contract test: `tooned check --format xml` on non-XML content honors the hint and reports `ParseFailed` passthrough (not a crash) — `crates/tooned-cli/tests/cli_check_xml.rs`.
- [x] T030 [P] [US2] MCP contract test: `tooned_detect` with `"format_hint": "xml"` returns `doc_type: "xml"` for valid XML and a `would_convert` boolean — `crates/tooned-cli/tests/mcp_detect_xml.rs`.

### Implementation for User Story 2

- [x] T031 [US2] Ensure `tooned check` parses `--format xml` and passes `ConversionOptions { format_hint: Some(DocType::Xml), .. }` to `inspect` (GREEN T028, T029).
- [x] T032 [US2] Ensure MCP `tooned_detect` maps `"xml"` format hint to `DocType::Xml` (GREEN T030).
- [x] T033 [US2] Run `cargo clippy --all-features --all-targets -- -D warnings` and `cargo fmt --all -- --check` on the CLI check and MCP modules; confirm T028–T030 all GREEN.

**Checkpoint**: User Stories 1 AND 2 both work independently.

---

## Phase 5: User Story 3 - Safe, Non-Breaking Integration with Existing v1 Surfaces (Priority: P2)

**Goal**: XML support does not break any existing v1 behavior; all v1 doctypes and integration surfaces continue to work identically.

**Independent Test**: Re-run v1 contract tests and confirm no regressions; add XML-only fixtures to the MCP and CLI tests.

### Tests for User Story 3 (write FIRST, confirm RED)

- [x] T034 [P] [US3] Regression test: re-run v1 `tooned-core/tests/roundtrip_proptest.rs`, `never_regression_proptest.rs`, `no_panic_proptest.rs`, and `multi_format_proptest.rs` against the updated `tooned-core` and confirm all pass unchanged.
- [x] T035 [P] [US3] Regression test: re-run v1 CLI/MCP contract tests (`cli_convert.rs`, `cli_check.rs`, `cli_pipe.rs`, `cli_wrap.rs`, `mcp_tools.rs`) and confirm all pass unchanged.
- [x] T036 [P] [US3] Integration test: `tooned convert file.xml --to json` decodes a converted TOON back to compact JSON and the output is structurally equivalent to the parsed XML value — `crates/tooned-cli/tests/cli_convert_xml.rs`.

### Implementation for User Story 3

- [x] T037 [US3] Verify that `tooned convert --to json` correctly calls `decode_toon` on the converted output (no code changes expected; it already does this for all doctypes) (GREEN T036).
- [x] T038 [US3] Run the full v1 test suite plus new XML tests: `cargo nextest run --all-features` and `cargo clippy --all-features --all-targets -- -D warnings` (GREEN T034, T035).

**Checkpoint**: All three user stories are independently functional and v1 is regression-free.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final performance, lint, and documentation gates.

- [x] T039 [P] Criterion benchmark: add an XML fixture to `crates/tooned-cli/benches/hot_path.rs` and confirm the 100 KiB XML record-list payload completes within the existing latency guardrail (low single-digit milliseconds).
- [x] T040 [P] `cargo deny check` against the final dependency set (including `quick-xml`); update `deny.toml` bans/exceptions if a new license issue appears.
- [x] T041 [P] Property test expansion: add XML fixtures to `crates/tooned-core/tests/multi_format_proptest.rs` so the multi-format round-trip/no-regression tests include `DocType::Xml`.
- [x] T042 Update `README.md` (or `CHANGELOG.md`) to list XML as a supported input format in v2.
- [x] T043 Full workspace release gate: `cargo fmt --all -- --check`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo nextest run --all-features`, `cargo deny check` all green on stable.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — start immediately.
- **Phase 2 (Foundational)**: Depends on Phase 1. BLOCKS all user stories.
- **Phase 3–5 (User Stories)**: All depend on Phase 2. They can run in parallel, but US3's regression tests should be run after US1 and US2 are complete.
- **Phase 6 (Polish)**: Depends on Phase 2 and all user stories.

### Within Each Phase

- Tests MUST be written and confirmed RED before the corresponding implementation task.
- Types/schema before logic; logic before CLI/hook wiring.
- Each phase's final "run clippy/fmt, confirm GREEN" task gates moving to the next phase.

### Parallel Opportunities

- All Setup tasks marked [P] (T001–T004) can run in parallel.
- All Foundational test tasks marked [P] (T006–T014) can run in parallel; T015–T020 are sequential.
- User Stories 1, 2, and 3 can run in parallel once Foundational is GREEN.
- All Polish tasks marked [P] (T039, T040, T041) are independent.

---

## Notes

- [P] tasks touch different files with no dependency on an incomplete task.
- [Story] labels map every Phase 3–5 task to its user story for traceability back to spec.md.
- No `cargo` commands are run during this planning phase; tasks are written for the implementation phase.
