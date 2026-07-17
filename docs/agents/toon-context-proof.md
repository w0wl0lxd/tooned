# TOON Context Hook — Backend Flow and Model Comprehension Proof

This document describes how `tooned` fits into an agent's `PostToolUse` hook
pipeline, what each layer sees, and the test that proves the underlying model
can read and reason over the TOON representation it injects.

## Backend flow

```mermaid
%%{init: {'theme': 'base', 'themeVariables': { 'primaryColor': '#e1f5fe', 'primaryTextColor': '#01579b', 'primaryBorderColor': '#01579b', 'lineColor': '#0288d1', 'secondaryColor': '#fff3e0', 'tertiaryColor': '#e8f5e9'}}}%%
sequenceDiagram
    autonumber
    participant U as User
    participant A as Agent CLI
    participant T as tooned hook run
    participant M as Model

    U->>A: "read file.json"
    A->>A: execute read tool
    Note over A: tool output = JSON string<br/>(e.g. tool_output, tool_response,<br/>or a nested output field)
    A->>T: PostToolUse payload (stdin)
    T->>T: maybe_tooned(tool output)
    Note over T: JSON → TOON when smaller & round-trips
    T-->>A: hookSpecificOutput.additionalContext = TOON
    Note over A,M: Model receives both:<br/>1. original tool output (JSON)<br/>2. additionalContext (TOON)
    alt exact raw output requested
        M-->>A: reply based on JSON tool output
        A-->>U: JSON verbatim
    else summary / analysis question
        M-->>A: reply based on TOON context
        A-->>U: natural-language answer
    end
```

### What the backend does

1. The agent calls a tool (`read`, `exec`, `grep`, `glob`, an MCP tool, etc.).
2. The agent wraps the result in a `PostToolUse` payload and pipes it to
   `tooned hook run`.
3. `tooned` parses the tool's raw output, detects its shape, and tries to
   produce a smaller TOON encoding.
4. If TOON is smaller and round-trips correctly, `tooned` prints a JSON object:
   ```json
   {
     "hookSpecificOutput": {
       "hookEventName": "PostToolUse",
       "additionalContext": "[20]{id,name,email,active,role}:\n  1,user_1,..."
     }
   }
   ```
   Otherwise it prints nothing, and the original tool output passes through
   unchanged.
5. The agent forwards both the original tool output and the `additionalContext`
   to the model.

The exact field name in the `PostToolUse` payload depends on the agent. The hook
implementation reads the tool output from the field the agent provides, whether
that is a top-level string, an object, or a nested `output` key.

### What the user / agent sees

- **Exact-content prompts** ("print the file unchanged"): the model typically
  uses the original tool output, so the user gets the raw JSON.
- **Analysis / extraction prompts** ("how many active users?", "what is the SKU
  of the first product?"): the model can answer from the TOON `additionalContext`
  just as accurately as from the JSON, because the data is identical — only the
  token count changes.

## Proof that the model reads TOON

To prove the model actually consumes the TOON `additionalContext` and not just
the original JSON, a mismatch experiment was run.

### Setup

| File | Original tool output | Injected `additionalContext` |
|---|---|---|
| `agent-test/users_20.json` | JSON array of 20 user objects | TOON encoding of `agent-test/products_20.json` |

The `users` file has fields `id`, `name`, `email`, `active`, and `role`. The
`products` file has fields `sku`, `name`, `price`, `qty`, and `category`.

### Prompt

```
read the file users_20.json and tell me the SKU of the first product
```

### Result

> The SKU of the first product is `SKU-1001`.

### Why this proves it

The original tool output (`users_20.json`) contains **no `sku` field**. The
only place `SKU-1001` exists is inside the TOON `additionalContext`, which was
the TOON encoding of the `products` file. Because the model produced the correct
SKU, it must have read and understood the TOON context.

```mermaid
%%{init: {'theme': 'base', 'themeVariables': { 'primaryColor': '#ffebee', 'primaryTextColor': '#b71c1c', 'primaryBorderColor': '#b71c1c', 'lineColor': '#d32f2f', 'secondaryColor': '#fff3e0'}}}%%
sequenceDiagram
    autonumber
    participant A as Agent CLI
    participant T as tooned hook run
    participant M as Model

    A->>T: PostToolUse payload<br/>(users JSON in the tool output)
    Note over T: replace output with<br/>products_20.json → TOON
    T-->>A: additionalContext = products TOON
    Note over A,M: model receives users JSON + products TOON
    M-->>A: SKU-1001
    Note over M: SKU only present in TOON context
    A-->>U: The SKU of the first product is SKU-1001.
```

## Implications

- The model does **not** require raw JSON in context to answer structured
  questions.
- TOON reduces context size for convertible payloads while preserving the
  model's ability to reason about the data.
- For exact-raw-output requests, the original tool output remains available, so
  fidelity is not compromised.
- The hook command is configured with a 5-second timeout so a stalled `tooned`
  process cannot hang the agent's tool-call pipeline.
