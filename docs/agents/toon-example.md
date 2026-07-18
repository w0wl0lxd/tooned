# Example: agents reason over TOON as if it were JSON

This is the worked example behind the README claim:

> [I tested this](toon-evidence.md) by having agents use `tooned` and then
> reason about the response. Even though the JSON bytes had been rewritten into
> TOON, the model still read and reasoned about the data as if it were the
> original JSON. It doesn't need the original syntax to understand what the
> data means.

## Setup

A `tooned` `PostToolUse` hook is wired into the agent. On each tool call (e.g.
`read`), it converts the JSON tool output into a smaller TOON encoding and
injects it as `additionalContext`. The original tool output is kept alongside
it; only the context the model reads is shrunk.

## What happened

1. **Same-file read.** `read users_20.json` produced a natural-language
   summary of the users — the model reasoned over the TOON `additionalContext`
   and answered as if it had the JSON.
2. **Mismatch proof (decisive).** The hook was set to inject the TOON of a
   *products* file as `additionalContext` while the agent `read` a *users* file
   (no `sku` field). Asking "what is the SKU of the first product?" returned
   `SKU-1001` — a value that existed **only** in the TOON context. The model
   had decoded the TOON on its own.
3. **Fidelity preserved.** An exact-raw-output prompt ("print the file
   unchanged") still returned the original JSON, because the hook keeps the
   original tool output next to the TOON context.

## Why this matters

The model consumed the TOON directly. No external decoder ran; the syntax
changed but the data model did not. TOON reduces context size for convertible
payloads without costing the model any comprehension.

## More

- Evidence with the model's own reasoning traces: [`toon-evidence.md`](toon-evidence.md)
- Backend flow + diagrams: [`toon-hook-flow.md`](toon-hook-flow.md)
- Cross-format decoding test + research: [`toon-decoding.md`](toon-decoding.md)
