# TOON decoding across formats

This document shows that a model, given a TOON-encoded context from a
`PostToolUse` hook, reads it into the underlying structure and answers as if it
had read the original JSON — with no external decoder involved.

## Core observation

When `tooned` injects TOON like this:

```toon
[20]{sku,name,price,qty,category}:
  SKU-1001,"Product 1",11.49,7,home
  SKU-1002,"Product 2",12.99,14,home
  ...
```

the model does **not** echo the TOON syntax back. It extracts the requested
field and replies in natural language or JSON values, e.g.:

> The SKU of the first product is `SKU-1001`.

This is an implicit, learned decode: the model sees the header `{sku,name,...}`,
maps it to a table schema, picks the first row, and returns the `sku` value.
No `toon → json` step runs inside the agent; the model performs the semantic
mapping itself. (We describe this as the model *reading* the TOON rather than
formally *proving* structural decoding — see the test caveats below.)

## Why this is expected

TOON is deliberately human-readable and structurally explicit: field names are
declared once, rows are comma-separated tuples, counts are explicit. That
pattern is well represented in pretraining data (CSV, Markdown tables, YAML
lists, SQL result sets, pandas output, logs). The model recognizes the
header/row convention and infers the schema without a bespoke parser.

Recent arXiv work supports this, with stated limits:

- **McMillan, 2026** — *Structured Context Engineering for File-Native
  Agentic Systems* ([2602.05447v2](https://arxiv.org/abs/2602.05447v2)):
  9,649 experiments across 11 models and 4 formats. Result: "format does not
  significantly affect aggregate accuracy (chi-squared=2.45, p=0.484)," but the
  same study reports *model-dependent* format sensitivities — particularly for
  open-source models. So comprehension is broadly format-independent in the
  aggregate, not universal per model.
- **Kutschka & Geiger, 2026** — *Notation Matters: A Benchmark Study of
  Token-Optimized Formats in Agentic AI Systems*
  ([2605.29676v2](https://arxiv.org/abs/2605.29676v2)): TOON cuts tokens up to
  18% with accuracy **within ~9 percentage points** of JSON; largest savings on
  tool schemas and tool results — exactly where `tooned` injects TOON. The
  accuracy cost is real, so "no loss" overstates it.
- **Matveev, 2026** — *Token-Oriented Object Notation vs JSON*
  ([2603.03306v1](https://arxiv.org/abs/2603.03306v1)): describes TOON as a
  serialization for structured data to LLMs, with "solid accuracy in LLM
  comprehension."
- **Dong et al., 2024** — *SpreadsheetLLM*
  ([2407.09025v2](https://arxiv.org/abs/2407.09025v2)): a compressed,
  structure-aware tabular encoding improves GPT-4 in-context learning by 25.6%
  and reaches 78.9% F1 — LLMs reason over heavily compressed tabular data when
  the logical structure is preserved.

## The mismatch test

1. The agent `read`s `users_20.json` (no `sku` field).
2. The hook is swapped to inject, as `additionalContext`, the TOON of
   `products_20.json`.
3. Prompt: `read users_20.json and tell me the SKU of the first product`.
4. Model answers: `The SKU of the first product is SKU-1001.`

Because `users_20.json` has no `sku`, the answer can only have come from the
TOON `additionalContext`. The model parsed the header and first row and
returned the `sku` value.

## Cross-format mismatch test

A universal mismatch hook ignored the real tool output and always emitted the
TOON of `products_20.json` as `additionalContext`. Same prompt each time:
`read <file> and tell me the SKU of the first product`.

| Original format | Model answer | Notes |
|---|---|---|
| XML, YAML, TOML, JSON5, NDJSON, CBOR, MessagePack | `SKU-1001` | Direct, unqualified |
| CSV, TSV, nested JSON, large uniform JSON, plain text | `SKU-1001` | Read TOON, but *hedged* — reconciled against the original output |

In every case the model extracted `SKU-1001` from the TOON context. The
"hedged" rows are the telling ones: the original output had enough
product-like structure that the model explicitly reconciled the two sources
before answering, rather than blindly matching. It held two structured
representations in context and reasoned about which answered the prompt.

**Caveat.** These results show the model *accessed* the TOON context to produce
`SKU-1001`; they are supporting evidence, not a controlled proof of structural
decoding. A formal proof would add derived multi-field computations, randomized
values, repeated trials, and captured raw outputs.

## Reading is easier than writing

These tests measure **comprehension** (reading TOON). Getting a model to
**generate** valid TOON is harder — it requires strict syntax (counts,
indentation, quoting). The arXiv work notes the same asymmetry: TOON as input
context holds up within the reported accuracy band; TOON as model output is
more error-prone. `tooned` is built around this — it only asks the model to
*read* TOON, never to *write* it.

## Reproduce

The universal mismatch hook below is portable: set `TOONED_BIN` to your built
`tooned` binary (e.g. `cargo build -p tooned-cli && export
TOONED_BIN=$PWD/target/debug/tooned`) and point `REPO_ROOT` at a checkout that
contains `agent-test/products_20.json`.

```python
#!/usr/bin/env python3
import json, os, subprocess, sys
from pathlib import Path

sys.stdin.read()  # discard the real tool output
repo_root = Path(os.environ.get("REPO_ROOT", "."))
products_text = (repo_root / "agent-test" / "products_20.json").read_text()
fake = json.dumps({"tool_response": {"output": products_text}})
tooned = os.environ.get("TOONED_BIN", "tooned")
result = subprocess.run(
    [tooned, "hook", "run", "--devin"],
    input=fake, text=True, capture_output=True, cwd=repo_root,
)
if result.stdout:
    print(result.stdout, end="")
```

> **Note on fixtures.** `agent-test/products_20.json` is generated test data,
> not committed to this repo. Regenerate it (or substitute any JSON array with
> a `sku` field) before running, then point `.devin/hooks.v1.json`'s
> `PostToolUse` command at this script and run e.g. `devin -p "read
> agent-test/records_20.xml and tell me the SKU of the first product"` —
> restoring the real `tooned hook run --devin` entry afterwards.

## More

- Findings + model explanations: [`toon-evidence.md`](toon-evidence.md)
- Backend flow diagrams: [`toon-hook-flow.md`](toon-hook-flow.md)
