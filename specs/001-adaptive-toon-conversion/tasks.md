---

description: "Task list for feature 001-adaptive-toon-conversion"
---

# Tasks: Adaptive TOON Conversion for AI Agent Tool-Call Context

**Input**: Design documents from `/specs/001-adaptive-toon-conversion/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Included and REQUIRED — constitution Principle IV (Test-First, NON-NEGOTIABLE)
mandates RED→GREEN TDD for every task, with `proptest` coverage for the two safety
invariants (round-trip fidelity, never-a-regression), not just example tests.

**Organization**: Tasks are grouped by user story (P1–P4 from spec.md) so each can be
implemented, tested, and delivered independently. The MCP server (FR-015) has no
dedicated user story in spec.md — its tasks live in the final Polish phase, unlabeled,
per the checklist format rules.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1 (P1, agent session), US2 (P2, standalone CLI), US3 (P3, index/stats), US4 (P2, safe coexistence install)
- File paths are exact and repo-relative to `/home/w0w/dev/tooned`

## Path Conventions

Cargo workspace, already scaffolded: `crates/tooned-core/`, `crates/tooned-index/`,
`crates/tooned-cli/`, each with its own `src/` and `tests/`. See plan.md's Project
Structure for the target module layout.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Expand the already-pushed 3-crate scaffold (stub `lib.rs`/`main.rs` only)
into the module layout plan.md specifies, and add the dependencies Phase 0 research
resolved that aren't in the scaffold yet.

- [X] T001 Add `sonic-rs = "0.5"` to `crates/tooned-core/Cargo.toml`; add `rmcp` (stdio server feature) to `crates/tooned-cli/Cargo.toml`; add `assert_cmd`, `predicates`, `criterion`, `tempfile` as dev-dependencies to `crates/tooned-cli/Cargo.toml`; add `criterion` as a dev-dependency to `crates/tooned-core/Cargo.toml`. Run `cargo deny check` to confirm no new bans/license violations.
- [X] T001b Workspace `Cargo.toml` `[workspace.lints.clippy]` and `clippy.toml` now deny `unwrap_used`/`expect_used`/`panic`/`todo`/`unimplemented`/`dbg_macro`/`get_unwrap`/`indexing_slicing`/`clone_on_ref_ptr`/`redundant_clone`/`manual_assert`/`disallowed_methods` (mirrors the vetanvil-backend/polymoney safety-lint standard, minus their Decimal/HFT-specific rules). Confirm `cargo clippy --all-features --all-targets -- -D warnings` still passes clean against the current scaffold before adding new stub code.
- [X] T002 [P] Create `crates/tooned-core/src/{detect.rs,parse.rs,shape.rs,convert.rs,error.rs}` as empty modules, declared via `mod` statements in `crates/tooned-core/src/lib.rs`
- [X] T003 [P] Create `crates/tooned-index/src/{schema.rs,scan.rs,sync.rs,gitignore.rs}` as empty modules, declared via `mod` statements in `crates/tooned-index/src/lib.rs`
- [X] T004 [P] Create `crates/tooned-cli/src/cli/{mod.rs,convert.rs,check.rs,pipe.rs,wrap.rs,index.rs,stats.rs}` and `crates/tooned-cli/src/hooks/{mod.rs,claude_code.rs,codex.rs,doctor.rs}` and `crates/tooned-cli/src/mcp/{mod.rs,server.rs}` as empty modules; wire a `clap::Parser` `Cli` struct with subcommand enum stubs in `crates/tooned-cli/src/main.rs` matching every command in `specs/001-adaptive-toon-conversion/contracts/cli.md`. `clippy::todo`/`unimplemented` are now denied (T001b) — stub bodies must be minimal working no-ops (e.g. `Ok(())` or a placeholder value) rather than `todo!()`/`unimplemented!()`
- [X] T005 [P] Create `crates/tooned-cli/benches/hot_path.rs` with an empty `criterion_group!`/`criterion_main!` skeleton; register it as `[[bench]] name = "hot_path" harness = false` in `crates/tooned-cli/Cargo.toml`
- [X] T006 Run `cargo build --all-features --all-targets`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo fmt --all -- --check` against the expanded skeleton; fix any warnings before proceeding (stub `todo!()` bodies are acceptable, dead-code/unused-import warnings are not)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: `tooned-core`'s full public API (`maybe_tooned`, `inspect`,
`decode_toon`) per `contracts/tooned-core-api.md` and `data-model.md`. Every user
story below calls into this — nothing in Phase 3+ can start until this is GREEN.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Tests for Foundational Phase (write FIRST, confirm RED)

- [X] T007 [P] Property test: for every input where `maybe_tooned` returns `Conversion::Toon`, `decode_toon(&text)` succeeds and is structurally equal (after normalizing to compact JSON) to the encoded value — `crates/tooned-core/tests/roundtrip_proptest.rs`
- [X] T008 [P] Property test: for every input where `maybe_tooned` returns `Conversion::Toon`, `report.toon_bytes < report.json_bytes` (never a regression) — `crates/tooned-core/tests/never_regression_proptest.rs`
- [X] T009 [P] Property test: `maybe_tooned` and `inspect` never panic for any `&[u8]` input, including invalid UTF-8, truncated multi-byte sequences, and adversarially deep nesting — `crates/tooned-core/tests/no_panic_proptest.rs`
- [X] T010 [P] Unit tests for format detection in `crates/tooned-core/src/detect.rs`: explicit `format_hint` is honored even when it conflicts with content; JSON/NDJSON/YAML/TOML/CSV/TSV are each correctly sniffed from representative fixtures; unrecognized content returns `None`
- [X] T011 [P] Unit tests for shape classification in `crates/tooned-core/src/shape.rs`: `uniformity_pct >= 0.9` → `UniformArrayOfObjects`; below threshold → `Irregular`; non-array root → `Scalar`; sampling caps at `K = 64` elements even for larger arrays
- [X] T012 [P] Unit test in `crates/tooned-core/src/convert.rs`: input exceeding `opts.max_input_bytes` returns `Passthrough { reason: InputTooLarge }` without any parser being invoked (assert via a parse-call-counting test double or by using an input that would panic every real parser if reached)
- [X] T013 [P] Unit test in `crates/tooned-core/tests/duplicate_keys.rs`: a JSON object with duplicate keys produces identical `maybe_tooned` output via the `sonic-rs` fast path and the `serde_json` fallback path (research.md #4 caveat)
- [X] T014 [P] Unit test in `crates/tooned-core/src/convert.rs`: a payload whose `toon_bytes` is smaller than `json_bytes` but by less than `opts.margin_pct` returns `Passthrough { reason: NotSmallerEnough { .. } }`, not `Toon`
- [X] T014b [P] Unit test in `crates/tooned-core/src/convert.rs`: a contrived payload whose round-trip check is forced to fail (e.g. via a test-only encode/decode seam or a crafted edge-case value) correctly downgrades to `Passthrough { reason: RoundTripMismatch }` rather than being surfaced as `Toon` (FR-008 negative path; complements T007's success-path property test)

### Implementation for Foundational Phase

- [X] T015 Implement `DocType`, `ConversionOptions`, `Conversion`, `ConversionReport`, `PassthroughReason`, `ShapeClass`, `ToonedError` per `data-model.md` in `crates/tooned-core/src/lib.rs` and `crates/tooned-core/src/error.rs`
- [X] T016 Implement `detect.rs`: hint-first detection, then leading-byte/line-shape sniffing for JSON, NDJSON/JSONL, YAML, TOML, CSV, TSV (GREEN T010)
- [X] T017 Implement `parse.rs`: parse into `serde_json::Value` — `sonic_rs::from_slice::<serde_json::Value>` for JSON above a size threshold on x86_64/aarch64, `serde_json::from_slice` otherwise; `serde_yaml`/`toml`/`csv` (BurntSushi, building `Vec<Map>` → `Value::Array`) for the other doctypes (GREEN T013)
- [X] T018 Implement `shape.rs`: `K = 64` sampling, per-element key-signature, `uniformity_pct` computation (GREEN T011)
- [X] T019 Implement `convert.rs::maybe_tooned`: `max_input_bytes` short-circuit → detect → parse → `toon_lsp::toon::encode` vs `serde_json::to_vec` (compact) comparison with `margin_pct` → round-trip check via `toon_lsp::toon::decode` → `Conversion` (GREEN T007, T008, T012, T014)
- [X] T020 Implement `convert.rs::inspect`: same detect+shape path as `maybe_tooned` without ever computing/returning TOON text
- [X] T021 Implement `error.rs::decode_toon`: wraps `toon_lsp::toon::decode`, mapping failures to `ToonedError`
- [X] T022 Audit `detect.rs`/`parse.rs`/`shape.rs`/`convert.rs` for any `unwrap`/`expect`/`panic!`/indexing that could panic on adversarial input; replace with explicit error handling that folds into `Passthrough` (GREEN T009)
- [X] T023 Run `cargo clippy --all-features --all-targets -- -D warnings` and `cargo fmt --all -- --check` on `tooned-core`; confirm T007–T014 all GREEN

**Checkpoint**: `tooned-core`'s public API is fully implemented and tested. All user stories below may now proceed, in any order or in parallel.

---

## Phase 3: User Story 1 - Automatic Token Savings During an Agent Session (Priority: P1) 🎯 MVP

**Goal**: Installing the Claude Code or Codex CLI hook transparently and safely
converts qualifying tool-call output to TOON when smaller, with zero developer action
per session.

**Independent Test**: Install the hook, trigger a tool call known to return a uniform
JSON array of objects, confirm the transcript shows the converted form; trigger a
control tool call returning non-JSON, confirm it is untouched.

### Tests for User Story 1 (write FIRST, confirm RED)

- [X] T024 [P] [US1] Integration test: `tooned hook run --claude-code` given a `PostToolUse` stdin payload (per `contracts/claude-code-hook.md`) with uniform JSON `tool_output` prints `hookSpecificOutput.updatedToolOutput` and exits 0 — `crates/tooned-cli/tests/claude_code_hook.rs`
- [X] T025 [P] [US1] Integration test: same scenario for `tooned hook run --codex` per `contracts/codex-hook.md` — `crates/tooned-cli/tests/codex_hook.rs`
- [X] T026 [P] [US1] Integration test (both hook variants): non-JSON, malformed, or oversized `tool_output` produces no stdout and exits 0 (hard passthrough)
- [X] T027 [P] [US1] Property test (both hook variants): the hook subcommand never panics for adversarial/malformed stdin JSON — `crates/tooned-cli/tests/hook_no_panic_proptest.rs`
- [X] T027b [P] [US1] Integration test: `tooned hook run --codex` given an input engineered to stall a naive implementation (e.g. a mocked slow parse path) still exits within its internal watchdog bound, well under Codex CLI's default hook timeout (per `contracts/codex-hook.md` — Codex does not blanket-guarantee fail-open, so this must be independently verified, not assumed) — `crates/tooned-cli/tests/codex_hook_watchdog.rs`
- [X] T028 [P] [US1] Integration test: `tooned hook install --claude-code` run twice produces no duplicate entry in `settings.json`'s `hooks.PostToolUse` array — `crates/tooned-cli/tests/hook_install_claude_code.rs`
- [X] T029 [P] [US1] Integration test: `tooned hook install --claude-code` writes matcher exactly `"Bash|Read|Grep|WebFetch|^mcp__"` per `contracts/claude-code-hook.md`
- [X] T030 [P] [US1] Integration test: `tooned hook install` aborts with a clear, non-zero-exit error and writes no config when the `tooned` binary cannot be resolved — `crates/tooned-cli/tests/hook_install_path_check.rs`
- [X] T031 [P] [US1] Integration test: `tooned hook install --codex [--mcp]` writes `.codex-plugin/plugin.json`, `hooks/hooks.json` (matcher `"Bash"`), and `.mcp.json` only when `--mcp` is passed — `crates/tooned-cli/tests/hook_install_codex.rs`
- [X] T031b [P] [US1] Integration test: `tooned hook install --codex` run twice produces no duplicate entry in `hooks/hooks.json` (Codex equivalent of T028; FR-016 applies to both agents, not just Claude Code) — `crates/tooned-cli/tests/hook_install_codex.rs`

### Implementation for User Story 1

- [X] T032 [US1] Implement `tooned hook run --claude-code` in `crates/tooned-cli/src/hooks/claude_code.rs`: parse stdin JSON, call `tooned_core::maybe_tooned` on `tool_output`, print `hookSpecificOutput.updatedToolOutput` JSON or nothing, always exit 0 (GREEN T024, T026, T027)
- [X] T033 [US1] Implement `tooned hook run --codex` in `crates/tooned-cli/src/hooks/codex.rs` with an internal watchdog timeout well under Codex CLI's default (per `contracts/codex-hook.md` — Codex does not blanket-guarantee fail-open) (GREEN T025, T026, T027, T027b)
- [X] T034 [US1] Implement `tooned hook install --claude-code [--scope user|project] [--mcp]` in `crates/tooned-cli/src/hooks/claude_code.rs`: PATH resolution check, idempotent JSON-merge (search by `command` string) into `hooks.PostToolUse` (GREEN T028, T029, T030)
- [X] T035 [US1] Implement `tooned hook install --codex [--mcp]` in `crates/tooned-cli/src/hooks/codex.rs`: PATH resolution check, write `.codex-plugin/plugin.json` + `hooks/hooks.json` (+ `.mcp.json` when `--mcp`) (GREEN T030, T031, T031b)
- [X] T036 [US1] After a successful `--codex` install, print the required `/hooks` trust-review instruction to stderr (per `contracts/codex-hook.md`)
- [X] T037 [US1] Run `cargo clippy --all-features --all-targets -- -D warnings` and `cargo fmt --all -- --check` on the hooks module; confirm T024–T031b all GREEN

**Checkpoint**: User Story 1 is independently functional — this is the MVP.

---

## Phase 4: User Story 2 - Standalone Command-Line Conversion (Priority: P2)

**Goal**: `convert`/`check`/`pipe`/`wrap` work correctly with no agent or hook involved.

**Independent Test**: Run each subcommand directly against a fixture file or piped
input; confirm correct conversion/passthrough and that source files are never mutated.

### Tests for User Story 2 (write FIRST, confirm RED)

- [X] T038 [P] [US2] Contract test: `tooned convert <file> --to toon` writes converted content to stdout — `crates/tooned-cli/tests/cli_convert.rs`
- [X] T039 [P] [US2] Contract test: `tooned convert <file> --to json` decodes a TOON file back to compact JSON — `crates/tooned-cli/tests/cli_convert.rs`
- [X] T040 [P] [US2] Contract test: after any `convert` invocation, the source file's bytes and mtime are unchanged (FR-005) — `crates/tooned-cli/tests/cli_convert.rs`
- [X] T041 [P] [US2] Contract test: `tooned check <file> [--precise]` prints doc type, shape class, and savings estimate, and produces no converted-output side effect — `crates/tooned-cli/tests/cli_check.rs`
- [X] T042 [P] [US2] Contract test: `tooned pipe` adaptively converts stdin to stdout, passthrough on non-JSON stdin — `crates/tooned-cli/tests/cli_pipe.rs`
- [X] T043 [P] [US2] Contract test: `tooned wrap -- <command>` mirrors the wrapped command's exit code and adaptively converts its captured stdout — `crates/tooned-cli/tests/cli_wrap.rs`

### Implementation for User Story 2

- [X] T044 [P] [US2] Implement `convert` subcommand in `crates/tooned-cli/src/cli/convert.rs` (GREEN T038–T040)
- [X] T045 [P] [US2] Implement `check` subcommand in `crates/tooned-cli/src/cli/check.rs`, including the `--precise` flag threading into `ConversionOptions.precise_tokens` (GREEN T041)
- [X] T046 [P] [US2] Implement `pipe` subcommand in `crates/tooned-cli/src/cli/pipe.rs` (GREEN T042)
- [X] T047 [US2] Implement `wrap` subcommand in `crates/tooned-cli/src/cli/wrap.rs` (subprocess spawn, captured-stdout conversion, exit-code passthrough) (GREEN T043)
- [X] T048 [US2] Run `cargo clippy --all-features --all-targets -- -D warnings` and `cargo fmt --all -- --check` on the cli module; confirm T038–T043 all GREEN

**Checkpoint**: User Stories 1 AND 2 both work independently.

---

## Phase 5: User Story 3 - Project-Wide Savings Visibility (Priority: P3)

**Goal**: `tooned index`/`index sync`/`stats` provide ranked savings visibility without
any live agent session.

**Independent Test**: Run `index` against a fixture project directory, confirm a
ranked report; modify/delete files and run `index sync`, confirm only changed files
are re-scanned and deleted files are pruned.

### Tests for User Story 3 (write FIRST, confirm RED)

- [X] T049 [P] [US3] Unit tests for SQLite schema creation (`meta`/`files`/`shapes`/`conversions` per `data-model.md`) — `crates/tooned-index/tests/schema.rs`
- [X] T050 [P] [US3] Integration test: full scan of a fixture project directory populates `files`/`shapes`/`conversions` correctly, respecting `.gitignore` via the `ignore` crate — `crates/tooned-index/tests/scan.rs`
- [X] T051 [P] [US3] Integration test: incremental sync skips re-hashing a file whose mtime is unchanged, re-classifies one whose content changed, and prunes a row for a deleted file — `crates/tooned-index/tests/sync.rs`
- [X] T052 [P] [US3] Integration test: first index creation appends `.tooned/` to the project's `.gitignore` (creating the file if absent); running index again does not duplicate the entry — `crates/tooned-index/tests/gitignore.rs`
- [X] T053 [P] [US3] Contract test: `tooned stats --top N` returns results ordered by `savings_pct` descending, limited to N — `crates/tooned-cli/tests/cli_stats.rs`
- [X] T054 [P] [US3] Contract test: `tooned index status`/`index show <file>` against a project with no existing index report "no index yet" gracefully (not a crash/panic) — `crates/tooned-cli/tests/cli_index.rs`

### Implementation for User Story 3

- [X] T055 [P] [US3] Implement `schema.rs`: table creation + `meta.schema_version` migration bootstrap in `crates/tooned-index/src/schema.rs` (GREEN T049)
- [X] T056 [US3] Implement `scan.rs`: directory walk via `ignore`, blake3 content fingerprinting, doctype detection + shape classification via `tooned_core`, persisted into `files`/`shapes`/`conversions` (GREEN T050)
- [X] T057 [US3] Implement `sync.rs`: stat-first incremental logic (mtime check before hash), prune rows for missing files (GREEN T051)
- [X] T058 [US3] Implement `gitignore.rs`: idempotent `.tooned/` append (GREEN T052)
- [X] T059 [P] [US3] Implement `index`/`index sync`/`index status`/`index show` subcommands in `crates/tooned-cli/src/cli/index.rs` (GREEN T054)
- [X] T060 [P] [US3] Implement `stats [path] [--top N]` subcommand in `crates/tooned-cli/src/cli/stats.rs` (GREEN T053)
- [X] T061 [US3] Run `cargo clippy --all-features --all-targets -- -D warnings` and `cargo fmt --all -- --check` on `tooned-index` and the cli index/stats modules; confirm T049–T054 all GREEN
- [X] T061b [P] [US3] Timed test/benchmark: `tooned index` full scan on a fixture project with 1,000+ files completes well under a minute; `tooned index sync` after touching only a handful of files completes markedly faster than the initial full scan (SC-005 — correctness alone is covered by T050/T051, this verifies the actual performance claim) — `crates/tooned-index/tests/scan_performance.rs`

**Checkpoint**: User Stories 1, 2, AND 3 all work independently.

---

## Phase 6: User Story 4 - Safe Installation Alongside Other Agent Tools (Priority: P2)

**Goal**: Installing, reinstalling, and uninstalling tooned's hook never disturbs a
pre-existing entry from another tool (e.g., rtk); `hook doctor` reports both correctly.

**Independent Test**: Pre-seed agent config with a foreign hook entry, install
tooned, confirm both entries present and correctly formed; reinstall, confirm no
duplication; uninstall, confirm only tooned's entry is removed.

### Tests for User Story 4 (write FIRST, confirm RED)

- [X] T062 [P] [US4] Coexistence test: pre-seed `settings.json` with a foreign `PostToolUse` entry, run `tooned hook install --claude-code`, assert the foreign entry is byte-for-byte unchanged and tooned's entry is appended — `crates/tooned-cli/tests/hook_coexistence_claude_code.rs`
- [X] T063 [P] [US4] Coexistence test: same scenario for `tooned hook install --codex` against a pre-existing unrelated `hooks.json` entry — `crates/tooned-cli/tests/hook_coexistence_codex.rs`
- [X] T064 [P] [US4] Uninstall test (both agents): uninstalling tooned removes only its own entry; a foreign entry remains intact — `crates/tooned-cli/tests/hook_uninstall.rs`
- [X] T065 [P] [US4] Uninstall test: uninstalling when tooned was never installed reports "nothing to remove" without erroring
- [X] T066 [P] [US4] Contract test: `tooned hook doctor` reports both tooned's and a foreign tool's entries correctly and performs no writes to either agent's config — `crates/tooned-cli/tests/hook_doctor.rs`
- [X] T067 [P] [US4] Contract test: `tooned hook status (--claude-code|--codex)` correctly reports installed vs. not-installed — `crates/tooned-cli/tests/hook_status.rs`

### Implementation for User Story 4

- [X] T068 [US4] Implement `tooned hook uninstall (--claude-code|--codex) [--scope user|project]` in `crates/tooned-cli/src/hooks/{claude_code,codex}.rs`: remove only the entry whose `command` matches tooned's own (GREEN T064, T065)
- [X] T069 [US4] Implement `tooned hook status (--claude-code|--codex)` in `crates/tooned-cli/src/hooks/mod.rs` (GREEN T067)
- [X] T070 [US4] Implement `tooned hook doctor` in `crates/tooned-cli/src/hooks/doctor.rs`: read-only report across both agents' configs listing every detected `PostToolUse`/hooks entry by `command`/`matcher` (GREEN T066)
- [X] T071 [US4] Harden the installer/uninstaller's config write against concurrent-write corruption: write to a temp file in the same directory, then atomically rename over the target (spec.md Edge Cases: concurrent installer runs) (GREEN T062, T063)
- [X] T072 [US4] Run `cargo clippy --all-features --all-targets -- -D warnings` and `cargo fmt --all -- --check` on the hooks module; confirm T062–T067 all GREEN

**Checkpoint**: All four user stories are independently functional — full v1 MVP scope complete.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: The MCP server (FR-015, no dedicated user story), the opt-in precise-token
mode, performance verification, and final release-readiness gates.

- [X] T073 [P] Contract tests for `tooned_convert`/`tooned_detect`/`tooned_decode` per `contracts/mcp-tools.md` — `crates/tooned-cli/tests/mcp_tools.rs`
- [X] T074 [P] Contract tests for `tooned_index_build`/`tooned_index_refresh`/`tooned_stats` — `crates/tooned-cli/tests/mcp_index_tools.rs`
- [X] T075 Implement `tooned mcp serve` in `crates/tooned-cli/src/mcp/server.rs` using `rmcp`'s stdio transport (`rmcp::transport::io::stdio`) and `#[tool_router(server_handler)]`/`#[tool(...)]` macros per `contracts/mcp-tools.md` (GREEN T073, T074)
- [X] T076 [P] Implement the opt-in `--precise` tokenizer-based savings estimate (via `tiktoken-rs`) behind `ConversionOptions.precise_tokens`, confirmed never invoked on the default hot path (FR-023) — `crates/tooned-core/src/convert.rs`
- [X] T077 [P] Implement and run the criterion benchmark + `--ignored` latency guardrail test (<5ms at 100 KiB for a uniform-array payload) — `crates/tooned-cli/benches/hot_path.rs`
- [X] T078 [P] Add `proptest` coverage for CSV/TSV and YAML/TOML detection+conversion parity beyond the JSON-focused Foundational-phase tests — `crates/tooned-core/tests/multi_format_proptest.rs`
- [X] T078b [P] Network-call guard test asserting no network-capable crate (e.g. `reqwest`, a `hyper` client) appears in `cargo tree` for the default build of `tooned-core`, `tooned-index`, or `tooned-cli` (FR-025 — v1 has zero telemetry/external calls; this makes that a regression-tested fact, not just a manual claim) — `crates/tooned-cli/tests/no_network_deps.rs`
- [X] T078c [P] Dependency-boundary guard test asserting `cargo tree -p tooned-core` contains none of `rusqlite`/`ignore`/`walkdir` (constitution Principle III, dependency-minimal core — currently guaranteed only by manual scaffold review; this makes a future accidental regression fail CI automatically) — `crates/tooned-core/tests/no_heavy_deps.rs`
- [X] T079 Run `cargo deny check` against the final dependency set (`sonic-rs`, `rmcp`, `rusqlite`, `tiktoken-rs`, etc.); update `deny.toml` bans/exceptions if a new AGPL or banned crate appears
- [X] T080 Manually validate `quickstart.md` end-to-end against a real built `tooned` binary (or automate via an `assert_cmd`-driven script) — install/convert/index/stats/hook install/hook doctor/hook uninstall
- [X] T080b [P] Contract test: `--help` output for the top-level `tooned` command and every subcommand is non-empty and documents its required flags (SC-006 — a new developer should be able to use tooned from `--help` alone, without external docs) — `crates/tooned-cli/tests/cli_help.rs`
- [X] T081 Update `README.md`'s "Status: pre-alpha, scaffold only" note to reflect the implemented v1 feature set
- [X] T082 Full workspace release gate: `cargo fmt --all -- --check`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo nextest run --all-features`, `cargo deny check` all green on stable

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. BLOCKS all user stories (US1–US4).
- **User Stories (Phase 3–6)**: All depend on Foundational completion. Independent of
  each other — may proceed in parallel or in priority order (US1 → US2/US4 → US3).
- **Polish (Phase 7)**: Depends on Foundational (for `tooned-core`) and on US1/US3
  (the MCP server's `tooned_index_*` tools call into `tooned-index`, built in US3).
  Does not depend on US2 or US4.

### User Story Dependencies

- **US1 (P1)**: Foundational only. No dependency on US2/US3/US4.
- **US2 (P2)**: Foundational only. No dependency on US1/US3/US4.
- **US3 (P3)**: Foundational only. No dependency on US1/US2/US4.
- **US4 (P2)**: Builds on the install/uninstall implementation US1 creates (T034, T035,
  T068, T069 are the same files) — in practice, implement US1 before US4, even though
  both are nominally "Foundational-only" per spec.md's independent-test framing. This is
  the one intentional sequencing exception; every other story pairing is truly parallel.

### Within Each Phase

- Tests MUST be written and confirmed RED before their corresponding implementation task.
- Types/schema before logic; logic before CLI/hook wiring.
- Each phase's final "run clippy/fmt, confirm GREEN" task gates moving to the next phase.

### Parallel Opportunities

- All Setup tasks marked [P] (T002–T005) can run in parallel once T001 lands.
- All Foundational test tasks marked [P] (T007–T014) can run in parallel; all
  Foundational implementation tasks are mostly sequential (T015 types block T016–T021).
- Once Foundational (Phase 2) is GREEN: US1, US2, US3 can all start in parallel; US4
  should start after US1's T034/T035 land (see sequencing exception above).
- All Polish tasks marked [P] are independent of each other; T075 depends on T073/T074.

---

## Parallel Example: Foundational Phase

```bash
# Launch all Foundational tests together (after T001-T006 Setup is done):
Task: "Property test: round-trip fidelity in crates/tooned-core/tests/roundtrip_proptest.rs"
Task: "Property test: never-a-regression in crates/tooned-core/tests/never_regression_proptest.rs"
Task: "Property test: no-panic in crates/tooned-core/tests/no_panic_proptest.rs"
Task: "Unit tests for format detection in crates/tooned-core/src/detect.rs"
Task: "Unit tests for shape classification in crates/tooned-core/src/shape.rs"
Task: "Unit test for max_input_bytes short-circuit in crates/tooned-core/src/convert.rs"
Task: "Unit test for duplicate JSON keys in crates/tooned-core/tests/duplicate_keys.rs"
Task: "Unit test for margin threshold in crates/tooned-core/src/convert.rs"
```

## Parallel Example: User Stories (post-Foundational)

```bash
# Three developers, three independent stories, all starting from the same Foundational checkpoint:
Developer A: Phase 3 (US1 — hooks)      T024-T037
Developer B: Phase 4 (US2 — CLI)        T038-T048
Developer C: Phase 5 (US3 — index)      T049-T061
# US4 (Phase 6) starts once Developer A's T034/T035 land.
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL — blocks all stories)
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: install the Claude Code hook locally, trigger a real tool
   call known to return uniform JSON, confirm the transcript shows converted output
5. This is a demoable MVP: automatic token savings in a live agent session

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. US1 → validate independently → MVP demo
3. US2 → validate independently → standalone CLI usable
4. US4 → validate independently → safe alongside rtk
5. US3 → validate independently → project-wide visibility
6. Polish (MCP server, benchmarks, docs) → v1 release-ready

### Parallel Team Strategy

Once Foundational is done: one developer per user story (US1, US2, US3 in parallel;
US4 slightly staggered behind US1 per the sequencing note above). Stories integrate
without touching each other's files, since each maps to distinct modules
(`hooks/`, `cli/{convert,check,pipe,wrap}.rs`, `tooned-index` + `cli/{index,stats}.rs`).

---

## Notes

- [P] tasks touch different files with no dependency on an incomplete task.
- [Story] labels map every Phase 3–6 task to its user story for traceability back to spec.md.
- Every safety-critical path (fail-safe passthrough, never-a-regression, round-trip
  fidelity, no-panic) has explicit property-test coverage, not just example tests —
  constitution Principle IV is NON-NEGOTIABLE.
- Per constitution guidance: commit after each task or logical group; subagents must
  not commit on their own initiative.
- Stop at any checkpoint to validate a story independently before continuing.

## Remediation Pass (post-`/speckit.analyze`)

Tasks T001b, T014b, T027b, T031b, T061b, T078b, T078c, T080b were added after the
initial `/speckit.tasks` run to close 8 of the 9 findings from the `/speckit.analyze`
cross-artifact review (C1, E1–E4, U1–U2, M1). The 9th finding (I2, spec.md SC-002
wording) was fixed directly in spec.md. The workspace `Cargo.toml`/`clippy.toml`
lint hardening (C1) was also applied directly rather than deferred, mirroring the
unwrap/expect/panic/todo/dbg_macro discipline already used in this developer's
vetanvil-backend and polymoney projects (their Decimal/HFT-specific rules were
deliberately not carried over — not relevant to tooned's domain).
