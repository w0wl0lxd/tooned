# CLI Exit Codes and Contracts

## Exit Codes

| Code | Meaning |
|------|---------|
| 0    | Success |
| 1    | Missing resource / usage error |
| 2    | I/O error or not-indexed error |
| 3    | Decode / parse error |
| 4    | Binary not on PATH (hook install only) |

## Subcommand Contracts

### `convert`
**Exit codes:** 0 (success), 2 (I/O error), 3 (decode error)

**Input:** File path or `-` for stdin. Reads are always read-only; never mutates the source file.

**Output:** Converted bytes to stdout or `--out` file. When `--out` points at the same file as input, uses atomic temp-file-then-rename to avoid truncation.

**Flag precedence:**
- `--to` forces direction (toon/json/onto/tron), bypassing adaptive decision
- `--format-hint` overrides content-sniffing for parser doc type
- `--margin` / `--max-bytes` override config file defaults
- `--dict` / `--no-dict`, `--auto-margin` / `--no-auto-margin`, `--entropy-gate` / `--no-entropy-gate` override config defaults when explicitly set
- `--protect` adds to config's protected field list

### `check`
**Exit codes:** 0 (always - "not convertible" is not a CLI error)

**Input:** File path or `-` for stdin.

**Output:** Human-readable report (doc type, shape, byte comparison, convertible y/n) or JSON with `--json`. I/O errors are reported but still exit 0.

**Flag precedence:**
- `--format-hint` overrides content-sniffing
- `--margin` / `--max-bytes` override config defaults
- `--precise` enables BPE-token-based savings measurement

### `pipe`
**Exit codes:** 0 (always - even I/O errors fall back to passthrough)

**Input:** stdin only.

**Output:** Converted bytes to stdout, or original bytes on passthrough. Best-effort write - broken pipe is not escalated.

**Flag precedence:**
- `--format-hint` overrides content-sniffing
- `--margin` / `--max-bytes` override config defaults

### `wrap`
**Exit codes:** Mirrors the wrapped command's exit code (or 1 if signal-killed)

**Input:** Command and arguments after `--`.

**Output:** Converted stdout to stdout, or original stdout on passthrough. stderr and exit code are passed through unchanged.

**Flag precedence:** None (uses default conversion options)

### `index`
**Exit codes:**
- `index [path]`: 0 (success), 2 (path not found)
- `index sync`: 0 (success), 1 (no existing index)
- `index status`: 0 (always)
- `index show <file>`: 0 (success), 2 (file not indexed / no index)
- `index compact`: 0 (success), 1 (no existing index)
- `index watch`: 0 (success), runs continuously

**Input:** Project path (default: current directory).

**Output:** Human-readable summary or JSON with `--json`.

**Flag precedence:**
- `--type-filter` restricts to specific document types
- `--exclude` adds gitignore-style glob exclusions

### `stats`
**Exit codes:** 0 (success), 1 (no existing index)

**Input:** Project path (default: current directory).

**Output:** Ranked savings report or JSON with `--json`.

**Flag precedence:**
- `--top` limits result count
- `--sort-by` changes ranking (savings/count/recency)
- `--type-filter` / `--exclude` filter input

### `diff`
**Exit codes:** 0 (success), 2 (input not converted / parse error)

**Input:** File path.

**Output:** Unified diff or JSON with `--json`.

**Flag precedence:**
- `--context` sets unified diff context lines

### `lint`
**Exit codes:** 0 (valid with/without warnings), non-zero on validation failure

**Input:** File path or `-` for stdin.

**Output:** Validation result or JSON with `--json`.

**Flag precedence:**
- `--max-bytes` sets size limit
- Config file overrides via `--config`

### `hook`
**Exit codes:**
- `hook run`: 0 (always - fail-safe guarantee)
- `hook install`: 0 (success), 1 (usage error), 2 (I/O error), 4 (binary not on PATH)
- `hook uninstall`: 0 (success), 1 (usage error), 2 (I/O error), 4 (binary not on PATH)
- `hook status`: 0 (success), 2 (usage error)
- `hook doctor`: 0 (always)

**Input:** Agent selector flags (`--claude-code`, `--codex`, `--devin`, `--droid`, `--opencode`, `--kilo`, `--pi`, or `--all`).

**Output:** Installation status or JSON report with `--json` for doctor.

**Flag precedence:**
- `--scope` (user/project) selects config location
- `--mcp` enables MCP server registration for supported agents

### `mcp`
**Exit codes:** 0 (success), 2 (I/O error)

**Input:** None (runs stdio server).

**Output:** MCP protocol over stdio.

**Flag precedence:** None

### `metrics`
**Exit codes:** 0 (success), non-zero on ledger I/O error

**Input:** User-global or project-scoped ledger (`--global` flag).

**Output:** Summary, breakdown, leaderboard, recent events, or export (JSON/CSV).

**Flag precedence:**
- `--global` reads user-global ledger instead of project
- `--since` / `--until` set date window
- `--metric` selects tokens vs bytes
- `--surface` filters to specific surface
- `--opportunity` includes index-discovered events

### `heatmap`
**Exit codes:** 0 (success), non-zero on ledger I/O error

**Input:** User-global or project-scoped ledger (`--global` flag).

**Output:** Terminal-rendered contribution calendar.

**Flag precedence:**
- `--global` reads user-global ledger
- `--all` spans full history instead of last year
- `--tui` launches interactive pager
- `--metric` selects tokens vs bytes
- `--surface` filters to specific surface
- `--since` / `--until` override date range

### `dashboard`
**Exit codes:** 0 (success), non-zero on ledger I/O error

**Input:** User-global or project-scoped ledger (`--global` flag).

**Output:** Interactive ratatui TUI dashboard.

**Flag precedence:**
- `--global` reads user-global ledger
- Window flags (`--since`, `--until`, `--metric`, `--surface`, `--opportunity`) filter data

### `completions`
**Exit codes:** 0 (success), non-zero on shell generation error

**Input:** Target shell via `--shell` flag.

**Output:** Shell completion script to stdout.

**Flag precedence:**
- `--shell` selects bash/zsh/fish/nushell/elvish/powershell

### `man`
**Exit codes:** 0 (success), non-zero on man page generation error

**Input:** None.

**Output:** Roff-formatted man page to stdout.

**Flag precedence:** None
