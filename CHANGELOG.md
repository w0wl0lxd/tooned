# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `tooned-core`: XML input support (detect + parse + adaptive TOON conversion). The XML
  sniffer is conservative (rejects HTML/DOCTYPE html and common HTML tags), the `quick-xml`
  parser uses streaming events with a depth guard, namespace stripping, and mixed-content
  handling, and `proptest` property tests now cover XML round-trip fidelity, never-a-regression,
  no-panic on adversarial/invalid UTF-8/HTML-like/truncated input, and cross-format parity with
  JSON. The `tooned` CLI and MCP server both accept `--format-hint xml` / `format_hint: "xml"`.
- `tooned-core`: dependency-minimal detect + adaptive-convert pipeline (`maybe_tooned`,
  `inspect`, `decode_toon`), embeddable in a hot `PostToolUse` hook path. Detects and
  converts JSON, NDJSON/JSONL, YAML, TOML, CSV, and TSV. Conversion always compares TOON
  against compact JSON and only returns TOON when it beats JSON by the configured margin
  (`ConversionOptions.margin_pct`) *and* survives a round-trip check — otherwise it falls
  back to `Passthrough` with a reason (`InputTooLarge`, `NotSmallerEnough`,
  `RoundTripMismatch`, and others). JSON parsing and serialization route through
  `sonic-rs` for all JSON/NDJSON input and output; `serde_json::Value` remains the
  interchange type because `toon_lsp::toon::encode`/`decode` require it. Duplicate
  object keys are resolved identically to the historical `serde_json` path. An opt-in
  `precise_tokens` mode (`tiktoken-rs`, `cl100k_base`) computes an exact
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
- Agent hook integrations for Droid (Factory AI), Devin, OpenCode, Kilo Code, and Pi:
  `tooned hook install --droid|--devin|--opencode|--kilo|--pi` writes each agent's native
  config (`.factory/hooks.json`, `.devin/hooks.v1.json` / `~/.config/devin/config.json`,
  `.opencode/plugins/tooned.ts`, `.kilo/plugin/tooned.ts`, `.pi/extensions/tooned.ts`),
  `hook run` handles each agent's `tool_response`/`tool_output` shape, and `hook uninstall`
  removes only tooned's own entry. Droid's generated hook carries a 5-second timeout and
  supports `tool_response` as either a string or an object; the TS plugins spawn `tooned` and
  mutate the agent payload in place. Closes #27, #28.
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
- Monorepo build tooling: `.config/rail.toml` for `cargo-rail` (workspace plan/run/unify/release
  orchestration) and a `justfile` with `fmt`, `check`, `clippy`, `test`, `doc`, `build`, and
  `validate` recipes. Workspace dependencies are now centrally declared and inherited via
  `workspace = true`, with `cargo-rail unify` keeping the graph consistent across targets.
  ([work-log](docs/agents/work-log/2026-07-16-cargo-rail-setup.md))
- **tooned-cli:** added `index compact` (SQLite WAL checkpoint), `index watch`
  (`notify`-based debounced filesystem watcher with `--debounce-ms` and
  `watch.debounce_ms` config support), and `diff` (compare original JSON with TOON
  round-trip using `similar`). ([work-log](docs/agents/work-log/2026-07-16-004-notify-index-watch.md))
- **tooned-cli:** added TOML configuration file support loaded from `--config`,
  `TOONED_CONFIG`, `$XDG_CONFIG_HOME/tooned/config.toml`, or `.tooned.toml`;
  CLI flags override config-file values. ([work-log](docs/agents/work-log/2026-07-16-003-post-review-optimizations.md))
- **tooned-detect:** added `heapster` as a dev-dependency and a zero-allocation
  test guardrail for the `detect` sniffing hot path across JSON, NDJSON, YAML,
  TOML, CSV, TSV, unstructured, and empty inputs. ([work-log](docs/agents/work-log/2026-07-16-005-heapster-zero-alloc.md))
- **tooned-convert / tooned-cli:** added prototype ONTO (`Object-Notation Tabular
  Output`) encoder/decoder for uniform arrays of flat objects and
  `tooned convert --to onto`. ([work-log](docs/agents/work-log/2026-07-16-003-post-review-optimizations.md))
- **tooned-convert / tooned-cli:** added prototype TRON (`Token-Reduced Object
  Notation`) record-stream encoder/decoder for flat objects and uniform arrays
  of flat objects, with `tooned convert --to tron` producing a class header and
  compact `A(value, value, ...)` record bodies, and `tooned convert --to json`
  decoding TRON back to compact JSON. ([work-log](docs/agents/work-log/2026-07-16-005-tron-record-stream-encoding.md))
- **tooned-cli / release pipeline:** added hidden `completions` and `man`
  subcommands and packaged generated shell completion scripts and a man page
  in release artifacts. ([work-log](docs/agents/work-log/2026-07-16-003-post-review-optimizations.md))
- **CI:** added Criterion benchmark and latency guardrail jobs, a `cargo vet`
  supply-chain audit gate with mozilla/google audit imports, and switched
  `security.yml` to direct `cargo audit`. ([work-log](docs/agents/work-log/2026-07-16-003-post-review-optimizations.md))
- **tooned-convert / tooned-detect / tooned-cli / tooned-index:** added MessagePack, CBOR,
  and JSON5 as input formats. `tooned-detect` now sniffs `.msgpack`/`.cbor` binary payloads
  and JSON5 text (comments, single-quoted strings, unquoted keys, trailing commas), while
  `tooned-cli` maps `.msgpack`, `.msg`, `.cbor`, and `.json5` extensions to the appropriate
  parser, and `--format-hint` / MCP `format_hint` accept `msgpack`, `cbor`, and `json5`.
- **tooned-convert / tooned-cli:** added streaming NDJSON/JSONL conversion for large inputs.
  `tooned convert --to tron` and the default adaptive path now stream NDJSON/JSONL inputs
  (when the format is explicit via `--format-hint ndjson` or the file extension is
  `.jsonl`/`.ndjson`). For `--to tron` forced, streaming writes to a temp file and then
  promotes it atomically for file output or copies to stdout for stdout output. For the
  default adaptive case, streaming writes to a temp file, compares output size vs input
  size using the margin check, and discards the temp and passthrough the original input
  if not smaller enough. On parse/IO error, falls back to passthrough of the original
  input (for stdin, spools stdin to a temp file first so it can be retried/copied). Only
  uses streaming when the input is large (above `max_input_bytes`) or the user explicitly
  forced `--to tron` with an NDJSON hint/extension. Small NDJSON inputs continue to use
  the bounded path. ([work-log](docs/agents/work-log/2026-07-16-006-streaming-ndjson.md))
- **tooned-cli:** added Devin CLI hook integration. `tooned hook run --devin` reads
  Devin's `PostToolUse` stdin shape (`tool_response.output`) and emits
  `hookSpecificOutput.additionalContext` when TOON conversion wins.
  `tooned hook install --devin [--scope user|project]` writes `.devin/hooks.v1.json`
  (project scope) or `~/.config/devin/config.json` (user scope) with a matcher covering
  `exec`, `read`, `edit`, `grep`, `glob`, and `mcp__` tools. `tooned hook uninstall --devin`,
  `tooned hook status --devin`, and `tooned hook doctor` reporting for Devin entries
  alongside Claude Code and Codex are also supported. ([work-log](docs/agents/work-log/2026-07-16-007-devin-hook.md))
- **tooned-cli:** added Droid (Factory AI) hook integration. `tooned hook run --droid`
  uses Droid's `hooks.PostToolUse` JSON format (`toolName` / `toolInput` / `toolOutput`)
  and mutates `toolOutput` in place when TOON conversion wins. `tooned hook install --droid`
  writes `.factory/hooks.json` (project) or `~/.factory/hooks.json` (user), `uninstall --droid`
  only removes tooned's own entries, and `status`/`doctor` report Droid installations.
  ([work-log](docs/agents/work-log/2026-07-17-008-droid-and-plugin-hooks.md))
- **tooned-cli:** added plugin-wrapped agent hooks for OpenCode, Kilo Code, and Pi.
  `tooned hook install --opencode|--kilo|--pi` writes a TypeScript plugin/extension file
  that calls `tooned hook run --<flag>` with a Claude-compatible `tool_output` payload,
  then mutates the agent's tool result only when TOON is smaller. `uninstall`, `status`,
  and `doctor` are supported for all three agents.
  ([work-log](docs/agents/work-log/2026-07-17-008-droid-and-plugin-hooks.md))
- **tooned-metrics** new crate and `tooned metrics`/`tooned heatmap` CLI views for the
  local-only, opt-out token-savings ledger. Records conversion outcomes from every
  `tooned` surface and renders a GitHub/Codex-style savings heatmap.
- **tooned-cli:** added short flags for the most common options (`-t/--to`, `-o/--out`,
  `-f/--format-hint`, `-m/--margin`, `-b/--max-bytes`, `-c/--config`, `-p/--precise`,
  `-n/--top`, `-j/--json`), hidden subcommand aliases (`c`/`convert`, `p`/`pipe`,
  `w`/`wrap`, `i`/`index`, `s`/`stats`, `d`/`diff`, `l`/`lint`, `h`/`hook`, `m`/`mcp`,
  etc.), and `tooned hook install/uninstall/status --all` for installing or removing
  hooks across every supported agent in one command.
- Added an agent-agnostic `tooned` Agent Skills standard `SKILL.md` at
  `.agents/skills/tooned/SKILL.md` so Claude Code, Codex, Devin, Cursor, Gemini CLI,
  and other compatible agents know when and how to use `tooned`.

### Fixed

- **tooned-core:** removed the redundant JSON-style structural-depth preflight
  from XML parsing; `quick-xml`'s own recursion limits and new adversarial
  tests now guard malformed input. ([work-log](docs/agents/work-log/2026-07-16-003-post-review-optimizations.md))

- **tooned-core:** XML entity reference resolution in text content: character references
  (`&#65;`, `&#x41;`) and predefined entities (`&lt;`, `&amp;`, `&gt;`, `&apos;`, `&quot;`)
  are now resolved to their Unicode equivalents; unknown custom entities remain literal
  `&name;`. ([work-log](docs/agents/work-log/2026-07-16-002-xml-conversion.md), bfdd12a)
- **tooned-core:** preservation of `xml:*` attributes (e.g., `xml:lang`, `xml:space`) in
  parsed XML output; custom entity references are preserved as literal text rather than
  being stripped. ([work-log](docs/agents/work-log/2026-07-16-002-xml-conversion.md), dd63632,
  d014002)
- **tooned-cli / tooned-index:** closed 001 Phase 8 convergence gaps: in-place
  `convert --out` reads the source fully and writes via a same-directory
  temp-file-then-rename so a crash cannot leave a partially-written source;
  `read_bounded` and `wrap` cap their initial allocation and use saturating
  arithmetic for `take` limits; `.gitignore` appends use `O_NOFOLLOW` on Unix and
  write via a same-directory temp-file-then-rename; `sync` includes `size_bytes`
  and keeps transient metadata-failure files in `seen`; `open_index` sets a
  5-second SQLite busy timeout; MCP handlers run conversion/detect/decode and
  index tools on `tokio::task::spawn_blocking`; `tooned check` prints size fields
  independently. (see [work-log](docs/agents/work-log/2026-07-15-001-convergence-and-wrap-hardening.md))
- **tooned-core:** removed the JSON-style structural-depth pre-check from YAML
  and TOML parsing; the parsers have their own recursion limits and the
  pre-check produced false positives on brackets inside YAML single-quoted
  strings/comments and TOML basic strings. (see [work-log](docs/agents/work-log/2026-07-15-001-convergence-and-wrap-hardening.md))
- **tooned-cli / tooned-core:** format-hint coverage tests for all CLI/MCP
  `parse_doc_type_hint` mappings (json, ndjson, yaml, toml, csv, tsv, xml). (d014002)
- **tooned-cli:** Devin CLI hook hardening: `tooned hook install --devin` now
  writes a 5-second `timeout` on the generated hook command, coerces malformed
  hooks files to the expected object/array shape instead of silently no-oping,
  and uses `%APPDATA%\devin\config.json` on Windows without requiring a home
  directory. Tests now match Devin's real `PostToolUse` payload (no
  `hook_event_name`) and cover status, doctor, coexistence, and concurrent
  install atomicity. ([work-log](docs/agents/work-log/2026-07-16-007-devin-hook.md))
- **tooned-cli:** `tooned hook install` no longer appends a duplicate
  `PostToolUse` entry when `tooned` later moves on `PATH` (e.g. after a
  reinstall to a new binary location). The merge now also collapses an existing
  entry by its tooned-owned command *suffix* (path-independent), so a prior
  entry with a different absolute prefix is deduplicated instead of duplicated.
- **tooned-index:** `tooned index sync` no longer prunes rows for files that
  are still present on disk but were briefly unreadable: unreadable walk
  entries, entries with no resolvable file type, non-UTF-8 paths, tooned's own
  internal files, and transient metadata failures are all recorded as `seen`
  rather than treated as deleted, so the prune pass keeps their rows intact.

- **tooned-cli:** `tooned dashboard` now fails gracefully with a clear error
  instead of panicking when stdout is not a terminal.
  ([work-log](docs/agents/work-log/2026-07-17-009-dashboard-tty.md))

### Security

- **tooned-cli:** `convert --to json` no longer materializes an arbitrarily
  large file in memory before its decoder's `max_input_bytes` cap is
  consulted. The decode direction now reads through the new bounded
  `read_input_bounded` (capped at `ConversionOptions.max_input_bytes`) instead
  of the unbounded `read_input`, closing a local denial-of-memory vector.
- **tooned-index:** hardened `.tooned/index.db` and `.gitignore` temp-file
  paths against symlink redirection by refusing to follow symlinks and using
  same-directory temp-file-then-rename writes. ([work-log](docs/agents/work-log/2026-07-16-003-post-review-optimizations.md))

### Known limitations

- Not yet published to crates.io or tagged as a release.
- `--scope user|project` is a Claude-Code-only concept; passing it with `--codex` is
  accepted but has no effect (Codex always writes the project-local `.codex-plugin/`
  bundle), and `tooned` warns on stderr when this happens rather than silently ignoring
  the flag.
