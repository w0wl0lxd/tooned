# tooned

[![CI](https://github.com/w0wl0lxd/tooned/actions/workflows/ci.yml/badge.svg)](https://github.com/w0wl0lxd/tooned/actions/workflows/ci.yml)
[![Security Audit](https://github.com/w0wl0lxd/tooned/actions/workflows/security.yml/badge.svg)](https://github.com/w0wl0lxd/tooned/actions/workflows/security.yml)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0--only-blue.svg)](LICENSE)

`tooned` watches the JSON-shaped data moving through an AI coding agent's tool
calls: API responses, database rows, config files read from disk. When TOON is
smaller, tooned swaps it in. There's nothing to configure or opt into per call,
and tooned never touches a source file. When TOON doesn't win, the agent sees
the original JSON, unchanged.

It runs as a Claude Code hook, a Codex CLI hook, a Devin CLI hook, an MCP
server, or a plain CLI you can pipe things through. It is not a replacement for
[rtk](https://github.com/rtk-ai/rtk). rtk rewrites and compresses command
output in general; `tooned` does one thing, re-encoding structured data, and
is built to sit alongside rtk in the same agent session without either tool
stepping on the other's configuration.

## Contents

- [Why](#why)
- [Install](#install)
- [Quick start](#quick-start)
- [Wiring it into an agent](#wiring-it-into-an-agent)
- [How the decision works](#how-the-decision-works)
- [The project index](#the-project-index)
- [Command-line interface](#command-line-interface)
- [Workspace layout](#workspace-layout)
- [Development](#development)
- [License](#license)

## Why

Here's a database query result, the kind of thing a tool call returns
constantly during an agent session:

```json
{"users":[{"id":1,"name":"Alice Chen","role":"admin","active":true},{"id":2,"name":"Bob Diaz","role":"editor","active":true},{"id":3,"name":"Carla Nunez","role":"viewer","active":false},{"id":4,"name":"Dev Patel","role":"editor","active":true}]}
```

246 bytes. Every row repeats the same four keys. TOON notices the array is
uniform, lifts the keys into a header once, and writes the rest as rows:

```toon
users[4]{id,name,role,active}:
  1,"Alice Chen",admin,true
  2,"Bob Diaz",editor,true
  3,"Carla Nunez",viewer,false
  4,"Dev Patel",editor,true
```

145 bytes for the same data, 41% smaller, and it decodes back to the exact
same JSON. The same logic applies to XML record lists:

```xml
<users>
  <user id="1" name="Alice Chen" role="admin" active="true"/>
  <user id="2" name="Bob Diaz" role="editor" active="true"/>
  <user id="3" name="Carla Nunez" role="viewer" active="false"/>
  <user id="4" name="Dev Patel" role="editor" active="true"/>
</users>
```

```toon
users[4]{@id,@name,@role,@active}:
  1,"Alice Chen",admin,true
  2,"Bob Diaz",editor,true
  3,"Carla Nunez",viewer,false
  4,"Dev Patel",editor,true
```

XML attributes map to `@`-prefixed keys in the JSONified intermediate, then
to the same TOON header format. tooned checks this on every tool call. A lone
scalar or a ragged, deeply nested object usually won't compress, so JSON stays
smaller. tooned tries both and ships the shorter one.

I tested this by having agents use `tooned` and then reason about the response.
Even though the JSON bytes had been rewritten into TOON, the model still read
and reasoned about the data as if it were the original JSON. It doesn't need
the original syntax to understand what the data means.

Example:

- A `read` of a JSON array of user objects produced a natural-language summary
  of the users, reasoning entirely over the TOON `additionalContext`.
- When the hook was set up to inject the TOON encoding of a *products* file
  as `additionalContext` while the agent `read` a *users* file, asking "what is
  the SKU of the first product?" returned `SKU-1001`, a value that existed only
  in the TOON context, not in the original `read` output.
- Exact-raw-output prompts ("print the file unchanged") still returned the
  original JSON, because the hook preserves the original tool output alongside
  the TOON context.

## Install

```bash
cargo install tooned
```

Or grab a prebuilt binary from the [releases page](https://github.com/w0wl0lxd/tooned/releases)
once one exists. v1 isn't tagged yet; see [Status](#status).

## Quick start

No agent required. tooned works as a plain filter:

```bash
# adaptive: TOON if it's smaller, untouched JSON if it isn't
curl -s https://api.example.com/users | tooned pipe

# same idea, wrapping a command instead of piping its output
tooned wrap -- gh pr list --json number,title,author

# force a direction
tooned convert data.json --to toon
tooned convert data.toon --to json

# see the verdict without producing output
tooned check data.json
# json, uniform array (4/4 rows), 246 -> 145 bytes (41% smaller), convertible: yes
```

`convert` never writes back to the source file. It only reads it and
prints somewhere else.

## Wiring it into an agent

```bash
# Claude Code: merges into settings.json, doesn't touch existing hook entries
tooned hook install --claude-code --scope project

# Codex CLI: writes a .codex-plugin/ bundle (hook + MCP server registration)
tooned hook install --codex --mcp

# Devin CLI: writes .devin/hooks.v1.json (project scope) or ~/.config/devin/config.json (user scope)
tooned hook install --devin --scope project

# Droid (Factory AI): writes .factory/hooks.json (project) or ~/.factory/hooks.json (user)
tooned hook install --droid --scope project

# OpenCode: writes .opencode/plugins/tooned.ts (project) or ~/.config/opencode/plugins/tooned.ts (user)
tooned hook install --opencode --scope project

# Kilo Code: writes .kilo/plugin/tooned.ts (project) or ~/.config/kilo/plugin/tooned.ts (user)
tooned hook install --kilo --scope project

# Pi: writes .pi/extensions/tooned.ts (project) or ~/.pi/agent/extensions/tooned.ts (user)
tooned hook install --pi --scope project
```

Codex requires an explicit trust step before a newly installed hook runs.
`tooned hook install --codex` tells you to run `/hooks` inside Codex CLI
after it finishes. Devin CLI loads hooks from `.devin/hooks.v1.json`
automatically; use `/hooks` to verify the loaded entries.

From here, an agent tool call (Bash/exec/Execute, Read/read, Grep/grep,
WebFetch, or any MCP tool) that returns JSON-shaped output gets inspected once
it finishes. If TOON wins, the agent sees the TOON version. If the payload is
ambiguous in any way (not JSON, too large, or it doesn't round-trip back to the
original), the agent sees exactly what the tool call returned. tooned never guesses.

Check what's installed, including hooks belonging to other tools, or back out:

```bash
tooned hook doctor
tooned hook uninstall --claude-code --scope project
```

Uninstalling only ever removes tooned's own entry.

Prefer MCP over a native hook, or your agent doesn't have hooks at all?

```bash
tooned mcp serve
```

exposes `tooned_convert`, `tooned_detect`, `tooned_decode`, and the index
tools below over stdio.

## How the decision works

For every payload tooned sees, it:

1. Sniffs the format (JSON, NDJSON/JSONL, YAML, TOML, CSV/TSV, or XML) from an
   explicit hint if one exists, otherwise from the content itself.
2. Parses it into a structured value.
3. Encodes that value as TOON and as compact (non-pretty) JSON, and compares
   the byte counts.
4. Returns TOON only if it beats JSON by more than a small margin (2% by
   default, so near-identical sizes don't flap back and forth between runs),
   *and* decoding that TOON back reproduces the original value exactly.

If a payload fails any check (a parse error, something past the 2 MiB cap, or a
round-trip that doesn't match), tooned returns the original bytes untouched.
It can't panic and makes no network calls, so the payload stays on your machine.

## The project index

Want to know where the savings actually are before wiring up a hook at all?

```bash
tooned index .          # scan the project, cache doc type + shape + savings per file
tooned index sync .     # re-scan only what changed since the last run
tooned stats --top 10   # biggest opportunities, ranked
```

The index lives at `.tooned/` in the scanned project; tooned adds it to
that project's `.gitignore` the first time it's created.

## Command-line interface

| Command | What it does |
|---|---|
| `tooned convert <file\|-> [--to toon\|json]` | One-shot conversion. Stdout by default, source untouched. |
| `tooned check <file\|-> [--precise]` | Reports format, shape, and savings. No output is produced; `--precise` measures against real LLM tokenization instead of byte count. |
| `tooned pipe` | stdin → adaptive conversion → stdout. |
| `tooned wrap -- <command>` | Runs `<command>`, adaptively converts its captured stdout. |
| `tooned index [path]` / `index sync` / `index status` / `index show <file>` | The `.tooned/` project index. |
| `tooned stats [path] [--top N] [--json]` | Ranked savings report from the index. `--json` emits machine-readable JSON. |
| `tooned hook install (--claude-code\|--codex\|--devin\|--droid\|--opencode\|--kilo\|--pi) [--scope user\|project] [--mcp]` | Installs the agent hook or plugin wrapper, idempotently. |
| `tooned hook uninstall / status / doctor` | Removes, checks, or audits hook installations. It never touches another tool's entries. |
| `tooned mcp serve` | Runs the MCP server over stdio. |

## Architecture

### Conversion pipeline

```mermaid
flowchart LR
    A[tool output<br/>JSON/NDJSON/YAML/TOML/CSV/TSV/XML/<br/>MessagePack/CBOR/JSON5] --> B{detect}
    B -->|format hint or sniff| C[parse into serde_json::Value]
    C --> D[structural-depth guard]
    D --> E[shape classify]
    E --> F[encode TOON]
    E --> G[encode compact JSON]
    F --> H{toon_bytes < json_bytes<br/>by margin_pct?}
    G --> H
    H -->|yes| I{decode TOON<br/>== original?}
    I -->|yes| J[return TOON]
    I -->|no| K[return Passthrough]
    H -->|no| K
    J --> L[agent sees TOON]
    K --> M[agent sees original]
```

### Workspace crate graph

```mermaid
graph TD
    tooned-cli --> tooned-core
    tooned-cli --> tooned-index
    tooned-cli --> tooned-metrics
    tooned-core --> tooned-convert
    tooned-core --> tooned-detect
    tooned-core --> tooned-json
    tooned-core --> tooned-xml
    tooned-core --> tooned-yaml
    tooned-core --> tooned-toml
    tooned-core --> tooned-csv
    tooned-convert --> tooned-toon
    tooned-cli --> tooned-convert
    tooned-metrics --> tooned-core
    tooned-index --> tooned-core
```

### Agent hook flow

```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant A as Agent CLI
    participant T as tooned hook run
    participant M as Model
    U->>A: read file.json
    A->>A: execute read tool
    A->>T: PostToolUse payload<br/>(Claude/Devin/Droid) or<br/>plugin wrapper callback<br/>(Codex/OpenCode/Kilo/Pi)
    T->>T: maybe_tooned(tool output)
    Note over T: JSON/NDJSON/XML/... → TOON when smaller & round-trips
    T-->>A: additionalContext = TOON (or nothing if not smaller)
    Note over A: original JSON + optional TOON context
    M->>A: reason over whichever context fits the prompt
    A-->>U: answer or verbatim output
```

## Workspace layout

```
crates/
├── tooned-core/    lib: detection + adaptive conversion, no I/O, no SQLite
├── tooned-index/   lib: the .tooned/ SQLite project index
├── tooned-metrics/ lib: local-only token-savings metrics ledger
└── tooned-cli/     bin "tooned": CLI, hook installers, MCP server, metrics views
```

`tooned-core` stays dependency-minimal because the hook loads it on every
qualifying tool call. The CLI, the hooks, and the MCP server all call the same
`tooned_core::maybe_tooned`, so the decision lives in one place.

## Development

```bash
cargo build --all-features --all-targets
cargo nextest run --all-features        # or: cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo fmt --all -- --check
cargo deny check
```

Stable Rust is the required toolchain and the hard CI/release gate; nightly
runs as a non-blocking canary. `unwrap`/`expect`/`panic!`/`todo!` are denied
workspace-wide. See [`Cargo.toml`](Cargo.toml) and [`clippy.toml`](clippy.toml).

With [Nix](https://nixos.org) and [direnv](https://direnv.net) installed,
`direnv allow` in the repo root gets you a shell with
[`mise`](https://mise.jdx.dev) and `rustup` on `PATH`, `cargo-nextest`/
`cargo-deny`/`cargo-audit` installed via mise, and `rustup` reading
`rust-toolchain.toml` for the Rust version. One command, nothing installed
outside the Nix store. `nix develop` works the same way without direnv.
`rust-toolchain.toml` stays the single source of truth for the Rust version
either way; `flake.nix` and `.mise.toml` both defer to it rather than pinning
their own.

Contribution guidelines, DCO sign-off, and commit conventions are in
[CONTRIBUTING.md](CONTRIBUTING.md).

## License

Dual-licensed under AGPL-3.0-only or a commercial license. See
[LICENSING.md](LICENSING.md) for which one applies to you, and
[COMMERCIAL-LICENSE.txt](COMMERCIAL-LICENSE.txt) for commercial terms.
