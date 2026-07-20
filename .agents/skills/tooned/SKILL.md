---
name: tooned
description: Use the tooned TOON re-encoder whenever you are working with structured data (JSON, YAML, TOML, CSV, XML, NDJSON, MessagePack, CBOR, JSON5) in this repository, when the user mentions tooned/TOON, or when you want to reduce token context from tool output. tooned losslessly compresses repetitive structured payloads so the model can reason over smaller context without losing the original data.
compatibility: Requires the `tooned` binary to be on PATH (cargo install tooned) or built from this repo (`cargo build -p tooned-cli`).
metadata:
  source: https://github.com/w0wl0lxd/tooned
---

# tooned — agent usage guide

`tooned` transparently re-encodes structured tool output into TOON when TOON is smaller and round-trips back to the original value exactly. It never mutates source files and never guesses: if conversion is not a clear win, the original bytes pass through unchanged.

## When to use this skill

- A tool returns JSON, NDJSON, YAML, TOML, CSV, XML, or binary MessagePack/CBOR.
- You are about to read a large structured file and only need to answer questions about its contents.
- The user asks about TOON, token savings, compressing output, or "should this be tooned?"
- You are installing or verifying agent hooks for this project.

## Quick decision tree

1. **Is a tooned hook installed for the current agent?**
   - Run `tooned hook status --<agent>` (or `--all`) to check.
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

## Useful commands

| Task | Command |
|------|---------|
| Adaptive stdin → stdout | `tooned pipe` |
| Wrap a command and convert its stdout | `tooned wrap -- gh pr list --json number,title,author` |
| Preview savings | `tooned check data.json` |
| Force conversion | `tooned convert data.json -t toon` |
| Decode TOON back to JSON | `tooned convert data.toon -t json` |
| Validate TOON | `tooned lint file.toon` |
| Scan project for savings | `tooned index .` |
| Re-scan changed files | `tooned index sync` |
| Show biggest opportunities | `tooned stats -n 10` |
| Install hook for one agent | `tooned hook install --devin` (also `--claude-code`, `--codex`, `--droid`, `--opencode`, `--kilo`, `--pi`) |
| Install hook for every agent | `tooned hook install --all` |
| Check installation status | `tooned hook status --all` |
| Audit all hook installations | `tooned hook doctor` |
| Run MCP server | `tooned mcp serve` |

## Important safety rules

- `tooned` never writes back to the source file unless the output path is the same file; even then it uses an atomic temp-file-then-rename.
- Agent protocols that can replace the tool result (`updatedToolOutput` for Claude Code/OpenCode/Kilo/Pi; `continue: false` + `decision: "block"` + `reason` feedback for Codex) put only TOON in the model's view. `tooned` does not use `additionalContext` because that would keep the original JSON and append the TOON, inflating total token count.
- For Devin and Droid, which only support `additionalContext` in `PostToolUse`, use `tooned wrap -- <cmd>` or `... | tooned pipe` when you need TOON-only output.
- For prompts that ask for the exact original file (e.g., "print the file unchanged") with a hook that replaces the tool result, the model will return the TOON text, not the original JSON. Rely on the original tool output or skip `tooned` when verbatim JSON is required.
- Do not try to generate TOON by hand; use `tooned convert` or `tooned pipe` so round-trip fidelity is verified.

## Hot-path toggles

`tooned pipe`, `tooned wrap`, and the installed agent hooks default to the zero-allocation `maybe_tooned_in` fast path, which skips the dictionary and key-folding tiers for lower latency. Set the matching environment variable to `0` to use the full `maybe_tooned` pipeline (dictionary, entropy, and critical-field tiers) instead:

- `TOONED_HOOK_ZERO_ALLOC=0`
- `TOONED_PIPE_ZERO_ALLOC=0`
- `TOONED_WRAP_ZERO_ALLOC=0`

## Default short flags

- `-t` = `--to`
- `-o` = `--out`
- `-f` = `--format-hint`
- `-m` = `--margin`
- `-b` = `--max-bytes`
- `-c` = `--config`
- `-p` = `--precise` (for `check`)
- `-n` = `--top` (for `stats`)
- `-j` = `--json` (for `stats`)

Subcommand aliases: `c` = `convert`, `p` = `pipe`, `w` = `wrap`, `i` = `index`, `s` = `stats`, `d` = `diff`, `l` = `lint`, `h` = `hook`, `m` = `mcp`.
