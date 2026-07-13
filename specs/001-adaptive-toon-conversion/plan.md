# Implementation Plan: Adaptive TOON Conversion for AI Agent Tool-Call Context

**Branch**: `001-adaptive-toon-conversion` | **Date**: 2026-07-13 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/001-adaptive-toon-conversion/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

tooned adaptively re-encodes JSON-shaped tool-call output (and standalone files) into
TOON whenever that measurably shrinks the payload versus compact JSON, with a hard
fail-safe passthrough on any doubt. Delivered as a 3-crate Rust workspace: `tooned-core`
(dependency-minimal detection/conversion, embeddable in a hook), `tooned-index`
(on-demand `.tooned/` SQLite project index), and `tooned-cli` (the single `tooned`
binary — CLI commands, Claude Code + Codex CLI hook installers, and an MCP server built
on `rmcp`). v1 covers JSON/NDJSON/JSONL, YAML, TOML, and CSV/TSV; ships with zero
external telemetry; and is designed to install safely alongside rtk's own hook entries.

## Technical Context

**Language/Version**: Rust, edition 2024, resolver 3; stable toolchain is the hard CI/release gate (nightly runs as a non-blocking canary only, per constitution Principle VI)
**Primary Dependencies**: `toon-lsp` 0.6 (crates.io — TOON encode/decode codec against `serde_json::Value`); `serde`/`serde_json` (preserve_order), `serde_yaml`, `toml`, `csv` (detection/parsing); `sonic-rs` (opportunistic SIMD JSON parse for larger payloads, x86_64/aarch64 only); `rusqlite` (bundled), `ignore`, `blake3` (tooned-index only); `clap` (CLI); `rmcp` (official `modelcontextprotocol/rust-sdk`, MCP server); `thiserror`/`anyhow`; `tracing`
**Storage**: `.tooned/<project>/index.db` — single-file SQLite index (tooned-index only; never touched by tooned-core's hot path)
**Testing**: `cargo nextest run --all-features` (stable-gated); `proptest` for the two safety invariants (round-trip fidelity, never-a-regression); `criterion` benchmarks + an `--ignored` latency guardrail test (<5ms at 100 KiB)
**Target Platform**: Linux, macOS, Windows — CLI/library, no OS-specific runtime dependency; prebuilt binaries for 5 targets already scaffolded in `release-binaries.yml`
**Project Type**: CLI + library workspace (3 crates), consumed both as a standalone binary and as agent-integration surfaces (Claude Code hook, Codex CLI hook, MCP server)
**Performance Goals**: Hook/CLI hot path (`tooned-core::maybe_tooned`) completes in a few milliseconds for typical (~100 KiB) payloads; `tooned index` completes well under a minute for a repository with thousands of files; `tooned index sync` is materially faster than a full scan by skipping unchanged files
**Constraints**: `tooned-core` MUST NOT depend on SQLite or perform file/directory I/O (constitution Principle III); conversion MUST NEVER mutate source files; on any doubt/error/oversized input, MUST hard passthrough without panicking (constitution Principle I); `max_input_bytes` default 2 MiB short-circuits before parsing; adaptive margin default 2%; zero external network calls in v1 (no telemetry, per clarification)
**Scale/Scope**: v1 doctypes: JSON, NDJSON/JSONL, YAML, TOML, CSV/TSV (XML deferred to v2, tracked as GitHub issue #1); v1 integration surfaces: standalone CLI, Claude Code hook, Codex CLI hook, agent-agnostic MCP server; broader editor/agent coverage explicitly out of scope for v1

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Check | Status |
|---|---|---|
| I. Fail-Safe Passthrough | `Conversion::Passthrough` is the default/error path everywhere; no `unwrap`/`expect`/`panic!` permitted on the `maybe_tooned` hot path or hook binary entrypoint (enforced by `clippy::unwrap_used`-style review, not just convention) | PASS (design commits to this; verified at Phase 1 + task-level tests) |
| II. Measurable Savings Only | `maybe_tooned` always compares `encode(&value)?.len()` vs `serde_json::to_vec(&value)?.len()` before returning `Toon`; round-trip check required before surfacing a conversion; margin default 2%, configurable | PASS |
| III. Dependency-Minimal Core | `tooned-core/Cargo.toml` (already scaffolded) has no `rusqlite`/`ignore`/`walkdir`; those live only in `tooned-index` | PASS (already true in scaffold; Phase 1 design preserves the boundary) |
| IV. Test-First | Every task in `tasks.md` will follow RED→GREEN; `proptest` covers the two safety invariants explicitly (not just example tests) | PASS (enforced at task-authoring time in `/speckit.tasks`) |
| V. Complementary Scope, Not Duplication | No git/gh/log filtering commands in the CLI surface (data model below stays JSON-shaped-conversion-only); installer JSON-merges by command string, never overwrites `hooks` key wholesale | PASS |
| VI. Stable-Gated CI, Nightly Canary | Already true in the pushed scaffold (`ci.yml` test-nightly job has `continue-on-error: true`; `release.yml` pinned to `@stable`) | PASS |
| VII. Dual License Integrity | `deny.toml` (scaffolded) already carries explicit `bincode`/`yaml-rust` bans and AGPL exceptions for all 4 workspace crates (toon-lsp, tooned-core, tooned-index, tooned-cli) | PASS |

No violations requiring Complexity Tracking justification.

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
crates/
├── tooned-core/            # lib: detection + adaptive conversion, zero I/O/SQLite
│   ├── src/
│   │   ├── lib.rs
│   │   ├── detect.rs        # format sniffing (JSON/NDJSON/YAML/TOML/CSV)
│   │   ├── parse.rs         # parse into serde_json::Value (sonic-rs / serde_json / serde_yaml / toml / csv)
│   │   ├── shape.rs         # ShapeClass + uniformity sampling
│   │   ├── convert.rs       # maybe_tooned, ConversionOptions, Conversion, round-trip check
│   │   └── error.rs         # ToonedError
│   └── tests/                # integration tests (round-trip + never-regression proptests live here)
├── tooned-index/           # lib: .tooned/ SQLite index
│   ├── src/
│   │   ├── lib.rs
│   │   ├── schema.rs        # meta/files/shapes/conversions tables + migrations
│   │   ├── scan.rs          # full scan via `ignore`, blake3 fingerprinting
│   │   ├── sync.rs          # incremental stat-then-hash sync, prune deleted
│   │   └── gitignore.rs     # auto-append .tooned/ to project .gitignore (FR-020)
│   └── tests/
└── tooned-cli/             # bin "tooned"
    ├── src/
    │   ├── main.rs
    │   ├── cli/              # convert, check, pipe, wrap, index, stats subcommands
    │   ├── hooks/             # claude_code.rs, codex.rs — installer/uninstaller/doctor, idempotent JSON-merge
    │   └── mcp/               # rmcp server: tooned_convert/detect/decode/index_build/index_refresh/stats
    ├── benches/               # criterion: hot-path latency guardrail (--ignored, <5ms @ 100KiB)
    └── tests/                 # CLI contract tests (assert_cmd), hook-installer idempotency tests

specs/001-adaptive-toon-conversion/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
└── tasks.md
```

**Structure Decision**: Reuses the 3-crate workspace already scaffolded and pushed
(`crates/tooned-core`, `crates/tooned-index`, `crates/tooned-cli`) — this is the
"Single project" shape adapted to a Cargo workspace rather than a single crate, since
the dependency-minimal-core constraint (Principle III) requires a real crate boundary,
not just an internal module boundary. No web/mobile structure applies; this is a
CLI + library product.

## Post-Design Constitution Re-Check

Re-evaluated after Phase 1 (data-model.md, contracts/, quickstart.md):

- **III. Dependency-Minimal Core**: confirmed by `contracts/mcp-tools.md`'s explicit
  rule that `tooned_convert`/`tooned_detect` MCP tools take `content` as a string
  parameter rather than a file path — this keeps even the MCP entrypoint from pulling
  filesystem I/O into the conversion path, extending Principle III's intent beyond just
  `tooned-core`'s own dependency graph.
- **I. Fail-Safe Passthrough**: `contracts/codex-hook.md` surfaced a real risk the
  original plan under-specified (Codex CLI does not blanket-guarantee fail-open) and
  the contract now requires an internal watchdog/timeout in the hook subcommand —
  strengthens rather than violates the principle.
- **V. Complementary Scope**: `contracts/claude-code-hook.md` and `contracts/codex-hook.md`
  both codify "search existing entries by `command` string before appending, never
  reorder/replace others" as an explicit installer contract, not just a stated intent.

No new violations surfaced by Phase 1 design. No Complexity Tracking entries required.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

*No violations — table intentionally omitted.*
