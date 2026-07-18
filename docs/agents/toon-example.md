# Example: agents reason over TOON as if it were JSON

This is the worked example behind the README claim:

> [I tested this](toon-evidence.md) by having agents use `tooned` and then reason about the response. Even though the JSON bytes had been rewritten into TOON, the model still read and reasoned about the data as if it were the original JSON.

## Setup

`tooned` is installed as a `PostToolUse` hook or used to wrap commands. On each tool call that returns JSON-shaped data it tries to produce a smaller TOON encoding. How the model receives the TOON depends on the agent protocol:

- `updatedToolOutput` — the TOON *replaces* the original tool output in place. Used by Claude Code, OpenCode, Kilo, and Pi.
- `continue: false` with `reason` feedback — the TOON replaces the model-visible tool result. Used by Codex CLI (Codex does not support `updatedToolOutput` for native tools).
- `additionalContext` is **not** emitted by `tooned`. Agents that only support `additionalContext` in `PostToolUse` (Devin, Droid) keep the original JSON in context and append the TOON, which inflates total token count rather than reducing it. For those agents, use command-level wrapping (`tooned wrap -- <cmd>` or `... | tooned pipe`) so the tool output itself is TOON before the agent sees it.

The example below was produced with an agent protocol that replaces the tool output with TOON, so the model saw only the TOON.

## The decisive test

A normal read of `agent-test/users_20.json` returns a JSON array of user objects (`id`, `name`, `email`, `active`, `role`). To force the model to use the TOON result, the tool output was replaced by the TOON encoding of `agent-test/products_20.json`.

The prompt:

```text
read the file users_20.json and tell me the SKU of the first product
```

The response:

```text
The SKU of the first product is SKU-1001.
```

`users_20.json` contains no `sku` field. The only place `SKU-1001` exists is the TOON tool result (the TOON of `products_20.json`). Because the model returned that value, it read and understood the TOON result.

## Exact-content requests

When the tool result is replaced with TOON, exact-content prompts ("print the file unchanged") return the TOON text, because the original JSON is no longer in that context item. If you need the original JSON verbatim, do not run `tooned` on that read, or use `tooned wrap -- <cmd>` in a way that keeps the original output available.

## What this supports

The model does not need the original JSON syntax to answer structured questions about the data. For payloads where TOON is smaller and round-trips, `tooned` can reduce context size without preventing the model from reasoning about the structure, provided the agent protocol replaces the native tool output with TOON rather than appending it as extra context.

This is supporting evidence, not a universal proof: it was observed in one tested configuration with one model, and the result is consistent with the format-independent comprehension reported in independent TOON/JSON literature (see [`toon-decoding.md`](toon-decoding.md)).

## More

- [`toon-evidence.md`](toon-evidence.md) — the full observation set.
- [`toon-context-proof.md`](toon-context-proof.md) — backend flow and the mismatch proof.
- [`toon-decoding.md`](toon-decoding.md) — cross-format decoding and research context.
