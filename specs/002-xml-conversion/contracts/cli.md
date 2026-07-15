# Contract: `tooned` CLI surface (v2 XML addition)

All commands are unchanged from v1. XML is supported by allowing `--format xml` (or `xml` as a value for the existing format hint option) and by auto-detecting XML content. No new subcommands are added.

| Command | XML behavior | Exit codes |
|---|---|---|
| `tooned convert <file\|-> [--to toon\|json] [--format xml]` | One-shot conversion; `--format xml` forces XML detection/parse. If XML is not detected or not a good TOON fit, the original is output unchanged. | 0 success; 2 input not found/unreadable; 3 decode failure when `--to json` on invalid TOON |
| `tooned check <file\|-> [--format xml]` | Dry-run: reports `doc_type: Xml`, shape, byte comparison, convertible y/n. | 0 always |
| `tooned pipe [--format xml] [--margin <pct>] [--max-bytes <n>]` | stdin → `maybe_tooned` → stdout. XML is auto-detected or forced by `--format xml`. | 0 always |
| `tooned wrap -- <command...>` | Captures stdout; if the content is XML and converts well, the converted content is printed; otherwise the original stdout is printed. | mirrors the wrapped command's exit code |

## Cross-cutting rules

- No new subcommands are added for XML.
- `--format xml` is optional; when omitted, `detect` uses the dedicated XML sniff path.
- `--format xml` overrides content-based detection, just as it does for other doctypes.
- All XML conversion follows the same fail-safe passthrough contract as v1 doctypes.
- The CLI does not fetch external DTDs or entities.
