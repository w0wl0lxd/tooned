# `tooned`

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0--only-blue.svg)](LICENSE)

Transparent TOON re-encoding for AI agent tool output. `tooned` watches structured payloads (JSON, NDJSON, YAML, TOML, CSV, XML, MessagePack, CBOR, JSON5) moving through agent sessions. When TOON is smaller and round-trips exactly, it replaces the original bytes. Otherwise the original passes through unchanged.


## What it does

A uniform array of objects repeats the same keys for every row:

```json
{"users":[{"id":1,"name":"Alice","role":"admin","active":true},...]}
```

TOON lifts the keys into a header once:

```toon
users[4]{id,name,role,active}:
  1,"Alice",admin,true
```

The model reads the TOON header/row structure directly; no `toon → json` conversion runs inside the agent. Even though the JSON bytes are rewritten into TOON, the model still read and reasoned about the data as if it were the original JSON. The mismatch test (reading one file while the hook injects the TOON of another) isolates this: when `users_20.json` (no `sku` field) is read with the TOON of `products_20.json` injected, a prompt asking for "the SKU of the first product" returns `SKU-1001`, which exists only in the injected TOON.

For agents that support tool result replacement (`updatedToolOutput` for Claude Code / OpenCode / Kilo / Pi; `continue: false` + `reason` for Codex), the model sees only the TOON for that tool call. For Devin / Droid (`additionalContext`-only), the hook passes through; use `tooned wrap -- <cmd>` or `... | tooned pipe` for TOON-only output.

## Install

```bash
cargo install tooned
```

Prebuilt binaries: see [releases](https://github.com/w0wl0lxd/tooned/releases) (v1 not tagged yet; see Status below).

Shell completions and man page:

```bash
tooned completions --shell bash > ~/.local/share/bash-completion/tooned.bash
tooned man | sudo tee /usr/local/share/man/man1/tooned.1 >/dev/null
```

## Quick start

```bash
# Adaptive: TOON only when smaller and exact round-trip
curl -s https://api.example.com/users | tooned pipe

# Wrap a command
tooned wrap -- gh pr list --json number,title,author

# Force direction
tooned convert data.json --to toon
tooned convert data.toon --to json

# Inspect without writing
tooned check data.json
```

`convert` reads from a file or stdin (`-`) and writes to stdout by default; `--out` redirects to a file. The source is never overwritten.

## Agent wiring

```bash
# All agents at once
tooned hook install --all --scope project

# Per agent (project scope writes `.devin/hooks.v1.json`, `.codex-plugin/`, etc.)
tooned hook install --claude-code --scope project
tooned hook install --codex --mcp
tooned hook install --devin --scope project
```

Codex requires an explicit trust step (`/hooks` inside Codex CLI) before a new hook runs. Devin loads `.devin/hooks.v1.json` automatically.

Check, audit, or remove:

```bash
tooned hook doctor
tooned hook uninstall --claude-code --scope project
```

## How the decision works

For every payload:

1. Sniff format (JSON, NDJSON, YAML, TOML, CSV/TSV, XML, MessagePack, CBOR, JSON5) from hint or content.
2. Parse into structured value.
3. Encode TOON and compact JSON; compare byte counts.
4. Accept TOON only if it beats JSON by the configured margin (default 2%) and `decode(encode(x)) == x` exactly.

If any check fails (parse error, >2 MiB cap, round-trip mismatch), the original bytes pass through.

## Project index

```bash
tooned index .          # scan, cache doc type + shape + savings per file
tooned index sync .     # re-scan changed files only
tooned stats --top 10   # biggest opportunities
```

The index lives at `.tooned/index.db`; `tooned` adds `.tooned/` to `.gitignore` on first creation.

## CLI reference

| Command | Purpose |
|---|---|
| `convert <file\|-> [--to toon\|json] [--out <path>]` | One-shot conversion |
| `check <file\|-> [--precise]` | Format, shape, savings; `--json` for machine output |
| `pipe` | stdin → adaptive conversion → stdout |
| `wrap -- <command>` | Run command, adaptively convert stdout |
| `index [path]` / `sync` / `status` / `show <file>` | `.tooned/` SQLite index |
| `stats [path] [--top N] [--json]` | Ranked savings report |
| `diff <file>` | Original vs TOON round-trip comparison |
| `lint <file\|->` | TOON validation (parse + round-trip + anti-patterns) |
| `hook install/uninstall/status/doctor [--all] [--scope user\|project] [--mcp]` | Agent hook management |
| `mcp serve` | MCP server over stdio (`tooned_convert`, `tooned_detect`, `tooned_decode`, index tools) |
| `metrics [summary\|breakdown\|top\|recent\|export\|reset]` | Local token-savings ledger |
| `heatmap` | Text calendar of savings |
| `dashboard` | Interactive `ratatui` dashboard |
| `completions --shell <bash\|zsh\|fish\|nushell\|elvish\|powershell>` | Shell scripts |
| `man` | Roff man page |

Exit codes: `0` success; `1` resource/operation error; `2` I/O or usage error; `3` decode/parse failure.

## Architecture

Conversion pipeline (same in CLI, hook, and MCP):

```
input → detect → parse → shape classify → encode TOON / encode compact JSON
        → compare bytes → round-trip check → return TOON or passthrough
```

Workspace crates:

```
crates/
├── tooned-core/    detection + adaptive conversion (dependency-minimal)
├── tooned-index/   `.tooned/` SQLite index
├── tooned-metrics/ local token-savings ledger
└── tooned-cli/     CLI, hooks, MCP server, metrics views
```

`tooned-core` stays minimal because it loads on every qualifying tool call. The CLI, hooks, and MCP server all call the same `maybe_tooned()` function.

## Development

```bash
cargo build --all-features --all-targets
cargo nextest run --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo fmt --all -- --check
cargo deny check
```

Stable Rust is the hard CI/release gate. `unwrap`/`expect`/`panic!`/`todo!` are denied workspace-wide (`clippy.toml`, `Cargo.toml`).

With [Nix](https://nixos.org) and [direnv](https://direnv.net): `direnv allow` sets up `mise`, `rustup`, `cargo-nextest`, `cargo-deny`, `cargo-audit`. `nix develop` works the same way. `rust-toolchain.toml` is the single source of truth; `flake.nix` and `.mise.toml` defer to it.

Contributions: see [`CONTRIBUTING.md`](CONTRIBUTING.md) (DCO sign-off, Conventional Commits).

## Status

v1 is not tagged yet. The conversion pipeline (`maybe_tooned`) is stable; the CLI surface, hook installers, and index schema are in active refinement. The evidence docs (`docs/agents/`) describe the mismatch test methodology; see [`toon-example.md`](docs/agents/toon-example.md) and [`toon-evidence.md`](docs/agents/toon-evidence.md). New validation should use agent CLI (`swe-1.7-max`, `glm-5.2` high) and avoid `additionalContext` (which was tainting earlier results by keeping original JSON in context).

## License

Dual-licensed: AGPL-3.0-only or commercial. See [`LICENSING.md`](LICENSING.md) and [`COMMERCIAL-LICENSE.txt`](COMMERCIAL-LICENSE.txt).
