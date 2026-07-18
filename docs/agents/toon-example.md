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
surfaces it to the model. How it is surfaced depends on the agent protocol:

- **`additionalContext`** (original tool output is preserved alongside TOON):
  Codex, Devin, Droid.
- **`updatedToolOutput`** (TOON *replaces* the tool output in place): Claude
  Code, OpenCode, Kilo, Pi.

The evidence below was produced with the Devin protocol, so the model saw both
the original JSON and the TOON `additionalContext`.

## What happened

1. **Same-file read.** `read users_20.json` produced a natural-language
   summary of the users — the model reasoned over the TOON `additionalContext`
   and answered as if it had the JSON.
2. **Mismatch test (the telling result).** The hook was set to inject the TOON
   of a *products* file as `additionalContext` while the agent `read` a *users*
   file (no `sku` field). Asking "what is the SKU of the first product?" returned
   `SKU-1001` — a value that existed **only** in the TOON context. The model had
   read the TOON to produce it.
3. **Fidelity preserved (Codex/Devin/Droid).** An exact-raw-output prompt
   ("print the file unchanged") still returned the original JSON, because those
   protocols keep the original tool output next to the TOON context.

## Why this matters

The model consumed the TOON directly. No external decoder ran; the syntax
changed but the data model did not. TOON reduces context size for convertible
payloads, and in the tested configuration the model answered structured
questions from TOON as well as from the JSON.

## More

- Evidence with the model's own explanations: [`toon-evidence.md`](toon-evidence.md)
- Backend flow + diagrams: [`toon-hook-flow.md`](toon-hook-flow.md)
- Cross-format decoding test + research: [`toon-decoding.md`](toon-decoding.md)
