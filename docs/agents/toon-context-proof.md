# TOON hook — backend flow

`tooned hook run` runs after a tool call. The payload shape and how TOON is returned depend on the agent protocol.

| Agent family | Tool output field | TOON surfaced as | Original output |
|---|---|---|---|
| Claude Code, OpenCode, Kilo, Pi | `tool_output` | `hookSpecificOutput.updatedToolOutput` | Replaced by TOON |
| Codex | `tool_response` | `continue: false` + `reason` feedback | Replaced by TOON |
| Devin, Droid | `tool_response` / `toolOutput` | None; hook passes through | Preserved |

For Devin / Droid, use `tooned wrap -- <cmd>` or `... | tooned pipe` for TOON-only output.

## What the backend does

1. Agent calls a tool (`read`, `exec`, `grep`, `glob`, MCP tool, etc.).
2. Agent wraps the result in a `PostToolUse` payload and pipes it to `tooned hook run`.
3. `tooned` detects format, parses it, tries to produce a smaller TOON encoding.
4. If TOON is smaller and round-trips, `tooned` prints a protocol-specific JSON:

- **Claude Code / OpenCode / Kilo / Pi:**
  ```json
  {
    "hookSpecificOutput": {
      "hookEventName": "PostToolUse",
      "updatedToolOutput": "[20]{id,name,email,active,role}:\n  1,user_1,..."
    }
  }
  ```

- **Codex:**
  ```json
  {
    "continue": false,
    "reason": "[20]{id,name,email,active,role}:\n  1,user_1,...",
    "hookSpecificOutput": {"hookEventName": "PostToolUse"}
  }
  ```

- **Devin / Droid:** nothing is printed. `additionalContext` is not emitted because it would keep the original JSON and inflate total token count.

5. The agent forwards the result to the model. With replacement protocols the model sees only the TOON. With Devin / Droid the model sees the original JSON unless the command was wrapped.

The mismatch test (reading `users_20.json` with `products_20.json` TOON injected) shows the model reads the TOON result directly. See [`toon-evidence.md`](toon-evidence.md) for the revised test protocol without `additionalContext`.
