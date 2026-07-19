# Example: agents read TOON as structured data

When `tooned` is installed as a `PostToolUse` hook with tool result replacement (`updatedToolOutput` for Claude Code / OpenCode / Kilo / Pi; `continue: false` + `reason` for Codex), the model receives only the TOON encoding. The original JSON is not in that context item.

For Devin / Droid (`additionalContext`-only), the hook passes through; the original JSON remains visible. Use `tooned wrap -- <cmd>` or `... | tooned pipe` for TOON-only output with those agents.

The mismatch test reads `users_20.json` (no `sku` field) while injecting the TOON of `products_20.json`. A prompt asking for "the SKU of the first product" returns `SKU-1001`, which exists only in the injected TOON — confirming the model consulted the TOON result.

Exact-output prompts (`"print the file unchanged"`) with replacement protocols return the TOON text, because the original JSON is no longer present in the context item.

See [`toon-evidence.md`](toon-evidence.md) for the revised methodology (without `additionalContext`) and [`toon-format-research.md`](research/toon-format-research.md) for when conversion succeeds or fails.
