# Contract: Claude Code `PostToolUse` hook

Verified against current Claude Code docs (research.md #1) — supersedes any earlier
unverified assumptions from the original product-context document.

## Registration (`settings.json`, merged idempotently by `tooned hook install --claude-code`)

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Bash|Read|Grep|WebFetch|^mcp__",
        "hooks": [
          { "type": "command", "command": "/absolute/path/to/tooned hook run --claude-code" }
        ]
      }
    ]
  }
}
```

- The installer MUST search the existing `hooks.PostToolUse` array for an entry whose
  inner `command` string already matches before appending a new one (FR-016).
- The installer MUST NOT replace or reorder any other array entry (FR-017) — this is
  what keeps installing alongside rtk's own `PostToolUse` (or other-event) entries safe.
- `--scope user` writes to the user-level settings file; `--scope project` writes to the
  project-level one; default is left as a task-level decision during `/speckit.tasks`
  (documented, not silently assumed) since it affects file path selection.

## Hook process I/O contract

**stdin** (JSON, one object per invocation):
```json
{
  "hook_event_name": "PostToolUse",
  "tool_name": "Bash",
  "tool_input": { "...": "tool's original arguments" },
  "tool_output": "the tool's result, as the hook receives it"
}
```
(Additional fields — `session_id`, `prompt_id`, `transcript_path`, `cwd`,
`permission_mode` — are present but not required by tooned's logic.)

**stdout on a convert decision** (exit 0):
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "updatedToolOutput": "<TOON-encoded text>"
  }
}
```

**stdout on passthrough**: nothing. Exit 0.

**On any internal error**: nothing to stdout, exit 0 (never non-zero for an internal
tooned problem — a non-zero exit is itself a form of "loud failure" the fail-safe
principle wants to avoid; log diagnostics to stderr or a log file instead, never to
stdout, since stdout is the channel Claude Code parses as hook output).

## Fail-open guarantee (platform-provided, confirmed by research.md #1)

Claude Code itself preserves the original `tool_output` if the hook process exits
non-zero, crashes, times out, or emits malformed JSON. tooned's own code MUST still
independently avoid panics (constitution Principle I) — this platform guarantee is
defense in depth, not a substitute for tooned's own safety.
