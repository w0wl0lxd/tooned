# Contract: `tooned` CLI surface

All commands are non-interactive/scriptable by default (research.md #5) and never
mutate the files they read (FR-005, FR-009).

| Command | Behavior | Exit codes |
|---|---|---|
| `tooned convert <file\|-> [--to toon\|json] [--out <file\|->]` | One-shot conversion; stdout by default. `--to` forces direction instead of the adaptive default. | 0 success; 2 input not found/unreadable; 3 decode failure when `--to json` on invalid TOON |
| `tooned check <file\|-> [--precise]` | Dry-run: prints doc type, shape class, byte-size comparison, convertible y/n. Never writes converted output. `--precise` additionally reports BPE-token-based savings (opt-in, per FR-023). | 0 always (a "not convertible" result is not a CLI error) |
| `tooned pipe [--margin <pct>] [--max-bytes <n>]` | stdin → `maybe_tooned` → stdout. Composable primitive. | 0 always (passthrough on any doubt, per FR-006/FR-007) |
| `tooned wrap -- <command...>` | Runs `<command...>`, captures stdout, feeds it through the same adaptive path, prints the result; stderr and exit code of the wrapped command are passed through unchanged. | mirrors the wrapped command's exit code |
| `tooned index [path]` | Full scan + classify + cache into `.tooned/index.db` at `path` (default: cwd). Appends `.tooned/` to `.gitignore` on first creation (FR-020). | 0 success; 2 path not found |
| `tooned index sync [path]` | Incremental: stat-first, re-hash/re-classify only on real change, prune deleted files (FR-021). | 0 success; 1 no existing index (suggests running `index` first) |
| `tooned index status [path]` | Reports index existence, file count, last scan time. | 0 always |
| `tooned index show <file>` | Reports the indexed record for one file. | 0 success; 2 file not indexed |
| `tooned stats [path] [--top N]` | Ranked savings-opportunity report from the index (FR-022). | 0 success; 1 no existing index |
| `tooned hook install (--claude-code\|--codex) [--scope user\|project] [--mcp]` | Idempotent installer (FR-016/FR-017); verifies the `tooned` binary resolves before writing (clarification). | 0 success; 4 binary not resolvable on PATH |
| `tooned hook uninstall (--claude-code\|--codex) [--scope user\|project]` | Removes only tooned's own entries (FR-018). | 0 success (including "nothing to remove") |
| `tooned hook status (--claude-code\|--codex)` | Reports whether tooned's hook is currently installed. | 0 always |
| `tooned hook doctor` | Reports all detected hook installations (tooned's and others', e.g. rtk) for both agents (FR-019). | 0 always |
| `tooned mcp serve` | Runs the MCP server over stdio (research.md #3). | non-zero only on transport-level startup failure |

## Cross-cutting rules

- No subcommand accepts or requires a config file to run with sensible defaults
  (margin 2%, max-bytes 2 MiB) — flags/env vars only override, never gate, basic use.
- Every subcommand that can encounter payload-driven ambiguity (`pipe`, `wrap`,
  `convert`) follows the same fail-safe passthrough contract as `tooned-core::maybe_tooned`
  (FR-006/FR-007) — a CLI-level error is reserved for I/O-level problems (file not
  found, unreadable), never for "this payload didn't parse as JSON."
