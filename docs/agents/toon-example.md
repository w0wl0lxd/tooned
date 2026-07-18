# Example: agents reason over TOON as if it were JSON

This is the worked example behind the README claim:

> [I tested this](toon-evidence.md) by having agents use `tooned` and then reason about the response. Even though the JSON bytes had been rewritten into TOON, the model still read and reasoned about the data as if it were the original JSON.

## Setup

`tooned` is installed as a `PostToolUse` hook. On each tool call that returns JSON-shaped data it tries to produce a smaller TOON encoding. Depending on the agent protocol the TOON is surfaced as:

- `additionalContext` — the original tool output is preserved and the TOON is added as extra context. Used by Codex, Devin, and Droid.
- `updatedToolOutput` — the TOON *replaces* the original tool output in place. Used by Claude Code, OpenCode, Kilo, and Pi.

The example below was produced with an agent that uses `additionalContext`, so the model saw both the original JSON and the TOON.

## The decisive test

A normal read of `agent-test/users_20.json` returns a JSON array of user objects (`id`, `name`, `email`, `active`, `role`). To force the model to use the TOON context, the hook was temporarily replaced by a mismatch hook: it left the original `read` output untouched but injected the TOON encoding of `agent-test/products_20.json` as `additionalContext`.

The prompt:

```text
read the file users_20.json and tell me the SKU of the first product
```

The response:

```text
The SKU of the first product is SKU-1001.
```

`users_20.json` contains no `sku` field. The only place `SKU-1001` exists is the injected TOON `additionalContext` (the TOON of `products_20.json`). Because the model returned that value, it read and understood the TOON context.

## Exact-content requests still get JSON

When the prompt asked to print the file unchanged, the model returned the original JSON. With `additionalContext` protocols the original tool output is still available, so the model can fall back to it for verbatim output.

## What this supports

The model does not need the original JSON syntax to answer structured questions about the data. For payloads where TOON is smaller and round-trips, `tooned` can reduce context size without preventing the model from reasoning about the structure.

This is supporting evidence, not a universal proof: it was observed in one tested configuration with one model, and the result is consistent with the format-independent comprehension reported in independent TOON/JSON literature (see [`toon-decoding.md`](toon-decoding.md)).

## More

- [`toon-evidence.md`](toon-evidence.md) — the full observation set.
- [`toon-context-proof.md`](toon-context-proof.md) — backend flow and the mismatch proof.
- [`toon-decoding.md`](toon-decoding.md) — cross-format decoding and research context.
