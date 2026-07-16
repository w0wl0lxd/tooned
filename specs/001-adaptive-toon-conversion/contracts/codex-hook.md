# Contract: Codex CLI `PostToolUse` hook + plugin bundle

Verified against current Codex CLI docs (research.md #2) — corrects the original
product-context document's unverified `shell`/`exec`/`local_shell` matcher guesses.

## Plugin bundle layout (installed by `tooned hook install --codex [--mcp]`)

```text
.codex-plugin/
├── plugin.json        # bundles both surfaces below
├── hooks/
│   └── hooks.json      # PostToolUse entry, matcher "Bash"
└── .mcp.json           # only written when --mcp is passed
```

`plugin.json` (sketch — exact field names finalized during implementation against the
live schema, not re-guessed here):
```json
{
  "hooks": "hooks/hooks.json",
  "mcpServers": ".mcp.json"
}
```

`hooks/hooks.json` (nested exactly like Claude Code's `hooks.json` shape — verified
against learn.chatgpt.com/docs/hooks; a flat `"command"` field directly on the matcher
entry, as an earlier draft of this sketch showed, is NOT the real schema and Codex CLI
will fail to load an entry shaped that way):
```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "/absolute/path/to/tooned hook run --codex"
          }
        ]
      }
    ]
  }
}
```

## I/O contract (verified against openai/codex `codex-rs/hooks/src/events/post_tool_use.rs`
and `output_parser::parse_post_tool_use()` — NOT the same shape as Claude Code's hook)

- **stdin**: the tool's raw result text is carried in a field named `tool_response` (NOT
  `tool_output` — that is Claude Code's field name only).
- **stdout**: Codex's output parser recognizes only `continue`/`decision`/`reason`/
  `stopReason` and `hookSpecificOutput.{hookEventName, additionalContext,
  updatedMCPToolOutput}` (`updatedMCPToolOutput` is explicitly documented as
  unsupported). There is no `updatedToolOutput` field in Codex's schema — unlike Claude
  Code, Codex has no mechanism to replace a regular tool's output in place. When tooned
  has a smaller TOON encoding to offer, it is surfaced via
  `hookSpecificOutput.additionalContext` (supplemental context alongside the original
  output) rather than a replacement, since that is the only field Codex's parser actually
  honors for this purpose.

## Trust review requirement

Non-managed hooks (everything `tooned hook install --codex` writes) require the
developer to run `/hooks` inside Codex CLI to review and trust the entry before it
executes (research.md #2, item 5); trust is pinned to the hook's content hash, so any
future change to tooned's own hook command re-triggers review. `tooned hook install
--codex` MUST print this instruction after writing the config — it is not optional
first-run polish, since without it the hook silently never fires.

## Fail-open guarantee (NOT platform-guaranteed — corrects the original plan)

Unlike Claude Code, Codex CLI's documentation does not blanket-guarantee fail-open
behavior for a hook process crash or timeout (research.md #2, item 4). Only specific
failure modes (e.g., a `PreToolUse` hook returning a malformed field) are documented as
fail-open. Consequently:
- tooned's own binary MUST independently guarantee it never panics, hangs past a
  bounded timeout, or exits in a way that could be interpreted as blocking (constitution
  Principle I) — this is not defense in depth here, it is the *only* guarantee.
- The hook subcommand implementation MUST set an internal watchdog/timeout well under
  any default Codex-side timeout, so tooned itself times out first and exits cleanly
  rather than relying on Codex CLI to kill a hung process gracefully.
