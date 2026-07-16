# Contract: MCP server tools (v2 XML addition)

Built on `rmcp` 2.x, stdio transport (reused from v1). XML support is additive: the existing tools accept `format_hint: "xml"` and the `tooned_detect`/`tooned_convert` outputs include `doc_type: "xml"` when XML is detected.

| Tool | XML-related input | XML-related output | Delegates to |
|---|---|---|---|
| `tooned_convert` | `{ "content": string, "format_hint"?: "xml", "margin_pct"?: number }` | `{ "converted": bool, "text": string, "report"?: ConversionReport }` | `tooned_core::maybe_tooned` |
| `tooned_detect` | `{ "content": string, "format_hint"?: "xml" }` | `InspectReport` with `doc_type: "xml"` when XML detected | `tooned_core::inspect` |
| `tooned_decode` | `{ "toon": string }` | `{ "value": <decoded JSON value> }` | `tooned_core::decode_toon` |

## Rules

- `format_hint` may be `"xml"` (lowercase). Other variants (`"XML"`) are accepted where the CLI parser already normalizes case.
- `tooned_convert`/`tooned_detect` continue to operate purely on the `content` string; no filesystem I/O.
- Tool errors (malformed XML, index not found) return a proper MCP tool-call error.
- The fail-safe passthrough contract applies: a malformed XML payload does not crash the server.
