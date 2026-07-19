# CLI contract

Exit codes (stable): `0` success; `1` resource/operation error; `2` I/O or usage error; `3` decode/parse failure.

Flag precedence (later wins): `ConversionOptions` defaults → `tooned.toml` / runtime config → explicit CLI flags (`--margin`, `--max-bytes`, `--format-hint`, etc.).

## Commands

| Command | Input | Output | Exit notes |
|---|---|---|---|
| `convert <file\|-> [--to toon\|json] [--out <path>]` | File or stdin (`-`) | Stdout by default; `--out` redirects. Source untouched. | `0` on conversion or passthrough. Passthrough is not an error. |
| `check <file\|-> [--precise]` | File or stdin | Human-readable; `--json` emits inspection object. Never writes bytes. | `0` unconditionally. |
| `pipe` | Stdin | Stdout | Adaptive conversion; passthrough when TOON does not win. |
| `wrap -- <command>` | Command stdout | Converted stdout | Mirrors wrapped command exit code. Passthrough when TOON does not win. |
| `index [path]` / `sync` / `status` / `show <file>` | Project root | `.tooned/index.db` | `status`: always `0`. `show`: `2` if file not indexed. `--json` and `--dry-run` supported. |
| `stats [path] [--top N] [--json]` | `.tooned/index.db` | Ranked savings; `--json` for machine output. | `1` if no index. |
| `diff <file>` | File | Human; `--json`: `{"equal": true\|false, "diff": "..."}` | `0` identical; `2` passthrough / no TOON; `3` round-trip mismatch. |
| `lint <file\|->` | File or stdin | Human; `--json`: `{"valid": true, "warnings": [...]}` | `0` valid (warnings still `0`); non-zero on parse/round-trip failure. |
| `hook install/uninstall/status/doctor [--all] [--scope user\|project] [--mcp]` | — | Human or `--json` | `status`: `0`. `doctor`: read-only, never writes. `--dry-run` on install/uninstall. |
| `mcp serve` | Stdio transport | `tooned_convert`, `tooned_detect`, `tooned_decode`, index tools | `0`. |
| `metrics [summary\|breakdown\|top\|recent\|export\|reset]` | `.tooned/metrics.db` or global ledger | Human; `export` supports `--format json\|csv\|prometheus\|otel` (default `json`), `--out <path>`, and `--push-url` with `--push-interval` (default `60`) via `curl`. | `summary` is default. `--metric tokens\|bytes`. `--since` / `--until` (default 365 days). |
| `heatmap` / `dashboard` | Metrics ledger | Text calendar (`heatmap`); interactive `ratatui` (`dashboard`). | Honor `--global`, `--metric`, `--since`, `--until`. |
| `completions --shell <shell>` / `man` | — | Shell script or roff man page to stdout. | `0`. No side effects. |
| `config validate [--config <path>]` | Config file | Human or parse error. | `0` valid; non-zero on parse/loading error. |
