# tooned

Transparent TOON re-encoding for AI coding agent tool-call context.

tooned detects JSON-shaped structured data flowing through AI coding agents'
tool-call context (API responses, DB rows, config files) and adaptively
re-encodes it as [TOON](https://github.com/w0wl0lxd/toon-lsp) whenever that's
measurably cheaper than compact JSON — never mutating source files, never
requiring hand-authored TOON.

Built on [`toon-lsp`](https://github.com/w0wl0lxd/toon-lsp)'s codec. Designed
to run alongside [rtk](https://github.com/rtk-ai/rtk), not replace it: rtk
compresses command output in general, tooned specializes in TOON-encoding
JSON-shaped payloads.

Status: pre-alpha, scaffold only. See `specs/` for the spec-kit pipeline
artifacts once generated.

## Workspace

- `crates/tooned-core` — doctype detection + adaptive conversion (no I/O)
- `crates/tooned-index` — the `.tooned/` SQLite project index
- `crates/tooned-cli` — the `tooned` binary: CLI, agent hooks, MCP server

## License

Dual-licensed under AGPL-3.0-only or a commercial license. See
[LICENSING.md](LICENSING.md).
