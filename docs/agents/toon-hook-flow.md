# Hook flow by agent protocol

`tooned hook run` operates as a `PostToolUse` command hook. The result format depends on the agent family.

| Agent | Tool result field | Replacement mechanism | Original output |
|---|---|---|---|
| Claude Code, OpenCode, Kilo, Pi | `tool_output` | `updatedToolOutput` in `hookSpecificOutput` | Replaced |
| Codex | `tool_response` | `continue: false` + `reason` feedback | Replaced |
| Devin, Droid | `tool_output` / `toolResponse` | None (passes through) | Preserved |

With replacement protocols (`updatedToolOutput` or `reason`), the model sees only the TOON. The original JSON is removed from that context item. Exact-output prompts return the TOON text; the original bytes are no longer visible.

For Devin / Droid, `additionalContext` is not emitted (it would keep the original JSON and inflate context). Use `tooned wrap -- <cmd>` or `... | tooned pipe` for TOON-only output.

The mismatch test (`users_20.json` + `products_20.json` TOON injection) confirms the model reads the TOON when replacement is active. See [`toon-evidence.md`](toon-evidence.md) for the revised protocol.
