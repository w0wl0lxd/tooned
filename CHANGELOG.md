# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `tooned-core`: dependency-minimal detect + adaptive-convert pipeline (`maybe_tooned`,
  `inspect`, `decode_toon`), embeddable in a hot `PostToolUse` hook path. Detects and
  converts JSON, NDJSON/JSONL, YAML, TOML, CSV, and TSV. Conversion always compares TOON
  against compact JSON and only returns TOON when it beats JSON by the configured margin
  (`ConversionOptions.margin_pct`) *and* survives a round-trip check — otherwise it falls
  back to `Passthrough` with a reason (`InputTooLarge`, `NotSmallerEnough`,
  `RoundTripMismatch`, and others). `sonic-rs` opportunistically accelerates JSON parsing
  above a size threshold on x86_64/aarch64 (falls back to `serde_json` elsewhere or below
  threshold), verified to resolve duplicate object keys identically to the `serde_json`
  path. An opt-in `precise_tokens` mode (`tiktoken-rs`, `cl100k_base`) computes an exact
  BPE-token savings estimate instead of the default byte-count estimate; never invoked on
  the default hot path. Two safety invariants are enforced by `proptest` across every
  supported doctype: JSON→TOON→JSON round-trip fidelity, and TOON is never returned unless
  it is actually smaller than compact JSON by the configured margin. A dedicated
  `no_panic_proptest` suite covers adversarial input (invalid UTF-8, truncated multi-byte
  sequences, deep nesting) for both `maybe_tooned` and `inspect`.
- `tooned-index`: the `.tooned/` project-local SQLite index (`rusqlite`, bundled).
  `tooned index` performs a full directory scan (respecting `.gitignore` via the `ignore`
  crate) that blake3-fingerprints, doc-type-detects, and shape-classifies every file into
  `files`/`shapes`/`conversions` tables; `tooned index sync` incrementally re-scans only
  files whose mtime changed (skipping a re-hash otherwise) and prunes rows for deleted
  files. First index creation idempotently appends `.tooned/` to the project's
  `.gitignore`. A scan of 1,000+ files completes well under a minute; incremental sync
  after touching a handful of files is markedly faster than a full re-scan.
- `tooned-cli` (bin `tooned`): standalone CLI surface `convert` (file/stdin → TOON or
  JSON, source files never mutated), `check [--precise]` (doc type, shape class, savings
  estimate, no side effects), `pipe` (adaptive stdin→stdout conversion), `wrap -- <cmd>`
  (mirrors the wrapped command's exit code, adaptively converts its captured stdout),
  `index` / `index sync` / `index status` / `index show <file>`, and `stats [--top N]`
  (ranked by `savings_pct` descending). Every subcommand's `--help` output is non-empty
  and documents its required flags.
- Claude Code `PostToolUse` hook integration: `tooned hook run --claude-code` reads the
  `tool_output` stdin field, replaces it in place via
  `hookSpecificOutput.updatedToolOutput` when a smaller TOON encoding exists, and always
  exits 0 (never blocks a tool call). `tooned hook install --claude-code [--scope
  user|project] [--mcp]` verifies the `tooned` binary resolves on `PATH` first, then
  idempotently merges a `PostToolUse` entry (matcher `Bash|Read|Grep|WebFetch|^mcp__`)
  into `settings.json`, leaving any pre-existing foreign hook entry byte-for-byte
  untouched. `tooned hook uninstall --claude-code` removes only tooned's own entry.
- Codex CLI hook integration: `tooned hook run --codex` reads the `tool_response` stdin
  field (Codex's own field name, distinct from Claude Code's `tool_output`) and, since
  Codex's real output parser has no field to replace a tool's output in place, surfaces a
  smaller TOON encoding via `hookSpecificOutput.additionalContext` instead. Runs the
  actual conversion on a worker thread behind an internal 3-second watchdog and always
  exits 0, since Codex CLI does not blanket-guarantee fail-open behavior for a hung/crashed
  hook process the way Claude Code does. `tooned hook install --codex [--mcp]` writes the
  `.codex-plugin/` bundle (`plugin.json`, `hooks/hooks.json` with matcher `Bash`, and
  `.mcp.json` only when `--mcp` is passed) and prints the required `/hooks` trust-review
  instruction to stderr. `tooned hook uninstall --codex` removes only tooned's own entry.
- Both installers write via a same-directory temp-file-then-atomic-rename, so a concurrent
  installer run never observes a partially-written config file.
- `tooned hook status (--claude-code|--codex)` (installed vs. not-installed) and
  `tooned hook doctor` (read-only report of every detected `PostToolUse`/hooks entry,
  tooned's own and any foreign tool's, across both agents — never writes).
- MCP server (`tooned mcp serve`, `rmcp` stdio transport): `tooned_convert`,
  `tooned_detect`, `tooned_decode`, `tooned_index_build`, `tooned_index_refresh`,
  `tooned_stats`.
- A criterion benchmark and an `--ignored` latency guardrail test confirm `maybe_tooned`
  completes in low-single-digit milliseconds against a ~100 KiB uniform-array-of-objects
  payload.
- Regression-tested dependency boundaries: no network-capable crate (e.g. `reqwest`, a
  `hyper` client) appears in any crate's dependency tree (v1 has zero telemetry/external
  calls), and `tooned-core` itself pulls in none of `rusqlite`/`ignore`/`walkdir`
  (constitution Principle III, dependency-minimal core).
- Dual licensing (AGPL-3.0-only + commercial), mirroring `toon-lsp`.

### Fixed

- **tooned-cli / tooned-index:** closed 001 Phase 8 convergence gaps: in-place
  `convert --out` no longer truncates the source; `read_bounded` and `wrap` cap
  their initial allocation and use saturating arithmetic for `take` limits;
  `.gitignore` appends use `O_NOFOLLOW` on Unix; `sync` includes `size_bytes` and
  keeps transient metadata-failure files in `seen`; `open_index` sets a 5-second
  SQLite busy timeout; MCP handlers run conversion/detect/decode and index tools
  on `tokio::task::spawn_blocking`; `tooned check` prints size fields
  independently. (see [work-log](docs/agents/work-log/2026-07-15-001-convergence-and-wrap-hardening.md))

### Known limitations

- Not yet published to crates.io or tagged as a release.
- `--scope user|project` is a Claude-Code-only concept; passing it with `--codex` is
  accepted but has no effect (Codex always writes the project-local `.codex-plugin/`
  bundle), and `tooned` warns on stderr when this happens rather than silently ignoring
  the flag.
