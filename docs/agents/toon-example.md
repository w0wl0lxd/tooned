# Example: agents reason over TOON as if it were JSON

When `tooned` is installed as a `PostToolUse` hook, a JSON tool response is rewritten into TOON. For agents that support tool result replacement (Claude Code, OpenCode, Kilo, Pi with `updatedToolOutput`, Codex with `continue: false` + `reason`), the model receives only the TOON. For Devin/Droid, which do not support replacement in `PostToolUse`, the hook passes through; use `tooned wrap -- <cmd>` or `... | tooned pipe` for TOON-only output. Emitting TOON as an `additionalContext` while leaving the original JSON in place would be an alternative implementation, but `tooned` avoids it because that would keep the original JSON in context and defeat the purpose of reducing context size.

## Mismatch test

The cleanest test reads `agent-test/users_20.json` while the hook replaces the tool result with the TOON of `agent-test/products_20.json`:

```text
read the file users_20.json and tell me the SKU of the first product
```

`users_20.json` has no `sku` field. The only source of `SKU-1001` is the TOON tool result, and the model returns it. This shows the model read the TOON result rather than simply echoing the original JSON.

For exact-output prompts (`print the file unchanged`) with replacement protocols, the model returns the TOON text because the original JSON is no longer in that context item.

See [`toon-evidence.md`](toon-evidence.md) for the full test results and [`toon-format-research.md`](research/toon-format-research.md) for when `tooned` does and does not convert a payload.
