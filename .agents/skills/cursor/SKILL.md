---
name: tooned-cursor
description: Use the tooned TOON re-encoder for Cursor agent tool output. Cursor supports PostToolUse hooks via shell commands.
compatibility: Requires the `tooned` binary on PATH (cargo install tooned) or built from this repo (`cargo build -p tooned-cli`).
metadata:
  source: https://github.com/w0wl0lxd/tooned
---

# tooned — Cursor agent usage guide

`tooned` transparently re-encodes structured tool output into TOON when TOON is smaller and round-trips back to the original value exactly. It never mutates source files and never guesses: if conversion is not a clear win, the original bytes pass through unchanged.

## Installation

Cursor uses shell-based `PostToolUse` hooks. Install the tooned hook:

```
tooned hook install --cursor --scope project
```

Or for user-level scope:

```
tooned hook install --cursor --scope user
```

## When to use this skill

- A tool returns JSON, NDJSON, YAML, TOML, CSV, XML, or binary MessagePack/CBOR.
- You are about to read a large structured file and only need to answer questions about its contents.
- The user asks about TOON, token savings, compressing output, or "should this be tooned?"

## Quick decision tree

1. **Is the tooned hook installed for Cursor?**
   - Run `tooned hook status --kilo` to check (Cursor uses the Kilo hook protocol).
   - If installed, tool outputs are converted automatically; proceed normally.
2. **No hook installed, but you have structured output in hand?**
   - Pipe it through `tooned pipe` before analyzing.
   - Or wrap the generating command with `tooned wrap -- <command>`.
3. **Want to preview savings without converting?**
   - `tooned check <file|->` or `tooned check -p <file|->` for token-level savings.
4. **Need one-shot conversion?**
   - `tooned convert data.json` (adaptive stdout)
   - `tooned convert data.json -t toon -o out.toon`
   - `tooned convert out.toon -t json`

## Important safety rules

- `tooned` never writes back to the source file unless the output path is the same file; even then it uses an atomic temp-file-then-rename.
- Cursor's PostToolUse hook replaces the tool result in place via `hookSpecificOutput.updatedToolOutput`. The model receives only TOON, not the original JSON.
- Do not try to generate TOON by hand; use `tooned convert` or `tooned pipe` so round-trip fidelity is verified.

## Hot-path toggles

The installed Cursor hook defaults to the zero-allocation `maybe_tooned_in` fast path. Set the environment variable to `0` to use the full pipeline instead:

- `TOONED_HOOK_ZERO_ALLOC=0`