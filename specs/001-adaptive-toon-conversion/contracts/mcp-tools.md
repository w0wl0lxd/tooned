# Contract: MCP server tools (`tooned mcp serve`)

Built on `rmcp` 2.x, stdio transport (research.md #3). Every tool call delegates to the
same `tooned-core`/`tooned-index` functions the CLI and hooks use — no parallel
conversion logic (constitution Principle V).

| Tool | Input | Output | Delegates to |
|---|---|---|---|
| `tooned_convert` | `{ "content": string, "format_hint"?: string, "margin_pct"?: number }` | `{ "converted": bool, "text": string, "report"?: ConversionReport }` | `tooned_core::maybe_tooned` |
| `tooned_detect` | `{ "content": string, "format_hint"?: string }` | `InspectReport` (doc type, shape class, estimated savings; no conversion performed) | `tooned_core::inspect` |
| `tooned_decode` | `{ "toon": string }` | `{ "value": <decoded JSON value> }` or an MCP tool error on invalid TOON | `tooned_core::decode_toon` |
| `tooned_index_build` | `{ "path": string }` | `{ "files_scanned": number, "gitignore_updated": bool }` | `tooned_index::scan` |
| `tooned_index_refresh` | `{ "path": string }` | `{ "files_rescanned": number, "files_pruned": number }` | `tooned_index::sync` |
| `tooned_stats` | `{ "path": string, "top_n"?: number }` | `{ "results": [{ "path": string, "savings_pct": number }] }` | index `conversions` query |

## Rules

- All tool errors (malformed input, index not found) return a proper MCP tool-call
  error, not a crash — an MCP client is treated the same as any other caller for the
  fail-safe principle: tooned itself never crashes the server process.
- `tooned_convert`/`tooned_detect` MUST NOT read from or write to the filesystem —
  they operate purely on the `content` string passed in the tool call, keeping the MCP
  surface as dependency-light as the hook path for the same reason (constitution
  Principle III extends conceptually to this entrypoint even though `tooned-cli` itself
  may depend on `tooned-index`).
