# TOON hook flow by agent protocol

`tooned hook run` is invoked as a `PostToolUse` command by the agent CLI. The payload shape and the way the TOON is returned depend on the agent protocol. The goal is always to put *only* the TOON in the model's view of a tool result; `tooned` does not use `additionalContext` because that would keep the original JSON and append the TOON, inflating total context.

## Protocol mapping

| Agent family | Tool output field | TOON surfaced as | Original tool output |
|---|---|---|---|
| Claude Code, OpenCode, Kilo, Pi | `tool_output` | `hookSpecificOutput.updatedToolOutput` | replaced by TOON |
| Codex | `tool_response` | top-level `continue: false` + `reason` feedback | replaced by TOON |
| Devin, Droid | `tool_response` / `toolOutput` | (none; hook passes through) | preserved |

For Devin and Droid, use command-level wrapping (`tooned wrap -- <cmd>` or `... | tooned pipe`) when TOON-only output is required.

## Flow

```mermaid
%%{init: {'theme': 'base', 'themeVariables': { 'primaryColor': '#e1f5fe', 'primaryTextColor': '#01579b', 'primaryBorderColor': '#01579b', 'lineColor': '#0288d1', 'secondaryColor': '#fff3e0'}}}%%
sequenceDiagram
    autonumber
    participant U as User
    participant A as Agent CLI
    participant T as tooned hook run
    participant M as Model

    U->>A: read file.json
    A->>A: execute read tool
    A->>T: PostToolUse payload
    T->>T: maybe_tooned(tool output)
    alt updatedToolOutput protocol (Claude Code / OpenCode / Kilo / Pi)
        T-->>A: hookSpecificOutput.updatedToolOutput = TOON
        Note over A,M: model sees only TOON
    else feedback replacement (Codex)
        T-->>A: continue=false, reason = TOON
        Note over A,M: model sees only TOON as tool result
    else no output replacement (Devin / Droid)
        T-->>A: (nothing)
        Note over A: original JSON passes through
    end
    M-->>A: structured answer
    A-->>U: answer
```

## What the user sees

- `updatedToolOutput` / feedback replacement protocols: the model sees only the TOON for that tool call. Exact-output prompts return the TOON or a summary of it; the original raw JSON is no longer in that context item. Fidelity for exact copies is therefore a protocol-level concern, not a `tooned` concern.
- Devin / Droid `PostToolUse` hooks: `tooned` prints nothing. The model sees the original JSON. To get TOON output, wrap the command with `tooned` so the tool output itself is TOON.

For the proof that the model can read the TOON result, see [`toon-context-proof.md`](toon-context-proof.md). For cross-format decoding, see [`toon-decoding.md`](toon-decoding.md).
