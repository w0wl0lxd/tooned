# `tooned` CLI contract

This document is the source of truth for `tooned` command-line exit codes,
input/output contracts, and flag precedence. It is aimed at script callers,
CI pipelines, and agent tooling that wraps `tooned`.

## Exit codes

`tooned` uses a small, stable set of exit codes:

| Code | Meaning | Typical triggers |
|---|---|---|
| `0` | Success | Command completed; `check` reports "not convertible" is still success |
| `1` | Resource error / operation failed | Missing index, hook install failure, binary not on `PATH` |
| `2` | I/O, usage, or not-found | Bad path, file not indexed, invalid CLI selector |
| `3` | Decode / parse failure | `tooned diff` round-trip mismatch, `lint` invalid TOON |

Specific commands may document narrower meanings; the table above is the
CLI-wide contract.

## Flag precedence

Configuration is layered in this order (later wins):

1. `ConversionOptions` defaults in code.
2. Values from `tooned.toml` / config files discovered at runtime.
3. Explicit command-line flags (`--margin`, `--max-bytes`, `--format-hint`, etc.).

So `--margin 5` always overrides `margin_pct = 2.0` in a config file.

## Command contracts

### `tooned convert <file|-> --to toon|json --out <path>`

- Reads input from a file path or stdin (`-`).
- Writes converted output to stdout by default; `--out` redirects to a file.
- Exits `0` on a successful conversion or passthrough. A passthrough is not
  an error -- `tooned` is declaring the original is already the better form.

### `tooned check <file|->`

- Dry-run diagnostic; never writes converted bytes.
- Exits `0` unconditionally (a "not convertible" answer is a valid answer).
- With `--json`, emits a single JSON object describing the inspection.

### `tooned pipe` and `tooned wrap -- <command>`

- `pipe`: stdin → adaptive conversion → stdout.
- `wrap`: runs the wrapped command, captures stdout, adaptively converts.
- Both pass through the original bytes unchanged when TOON does not win.
- `wrap` mirrors the wrapped command's exit code.

### `tooned index [path]` / `index sync` / `index status` / `index show <file>`

- Creates and updates `.tooned/index.db` under the project root.
- `index status` always exits `0`, even when no index exists.
- `index show <file>` exits `2` if the file is not indexed.
- `--json` is supported on `scan`, `sync`, `status`, `show`, and `compact`.
- `--dry-run` is supported on `scan`, `sync`, and `compact`.

### `tooned stats [path]`

- Reads from `.tooned/index.db`.
- Exits `1` when no index exists.
- `--json` emits a JSON array of ranked entries.

### `tooned diff <file>`

- Compares the original structured value with the TOON round-trip.
- Exits `0` when identical.
- Exits `2` when the input is not convertible to TOON (passthrough) or TOON is
  not produced.
- Exits `3` when the TOON round-trip produces a different structured value.
- `--json` emits `{"equal": true|false, "diff": "..."}`.

### `tooned lint <file|->`

- Validates a TOON file: parse, round-trip, and anti-pattern checks.
- Exits `0` on valid TOON (warnings are still `0`), non-zero on parse/round-trip
  failures.
- `--json` emits `{"valid": true, "warnings": [...]}`.

### `tooned hook install|uninstall|status|doctor`

- `install` and `uninstall` support `--all` to operate on every supported agent.
- `--dry-run` is supported on `install` and `uninstall`.
- `status` always exits `0`.
- `doctor` is read-only and never writes.
- `doctor` defaults to human-readable output; `--json` emits JSON.

### `tooned metrics [summary|breakdown|top|recent|export|reset]`

- Reads from `.tooned/metrics.db` (project) or the user-global ledger.
- `summary` is the default subcommand.
- `--metric tokens|bytes` controls the displayed unit.
- `--since` and `--until` filter the window; the default window is the last
  365 days.
- `export` supports `--format json|csv|prometheus|otel` (default `json`) and
  writes to stdout or `--out <path>`. With `--push-url`, it pushes the formatted
  metrics to the given endpoint on `--push-interval` seconds (default `60`)
  using `curl` and loops until interrupted.

### `tooned heatmap` / `tooned dashboard`

- `heatmap` renders a text calendar from the metrics ledger.
- `dashboard` launches an interactive ratatui dashboard.
- Both honor `--global`, `--metric`, `--since`, and `--until`.

### `tooned completions --shell <shell>` / `tooned man`

- `completions` writes a shell completion script to stdout.
- `man` writes the roff man page to stdout.
- Both exit `0` and produce no side effects.

### `tooned config validate [--config <path>]`

- Loads and validates a `tooned` configuration file.
- Exits `0` when the file parses correctly; non-zero with a parsing/loading
  error.
