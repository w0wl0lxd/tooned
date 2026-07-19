# TOON decoding across formats

This document explains how a model, given a TOON-encoded context from a `PostToolUse` hook, can read it into the underlying structure and answer as if it had the original JSON.

## Core observation

When `tooned` injects TOON like this:

```toon
[20]{sku,name,price,qty,category}:
  SKU-1001,"Product 1",11.49,7,home
  SKU-1002,"Product 2",12.99,14,home
  ...
```

the model does not echo the TOON syntax back. It extracts the requested field and replies in natural language or JSON values, e.g.:

> The SKU of the first product is `SKU-1001`.

This is an implicit, learned decode: the model sees the header `{sku,name,...}`, maps it to a table schema, picks the first row, and returns the `sku` value. No `toon → json` step runs inside the agent; the model performs the semantic mapping itself. We describe this as the model *reading* the TOON rather than formally *proving* structural decoding.

## Why this is expected

TOON is deliberately human-readable and structurally explicit: field names are declared once, rows are comma-separated tuples, counts are explicit. That pattern is well represented in pretraining data (CSV, Markdown tables, YAML lists, SQL result sets, pandas output, logs). The model can recognize the header/row convention and infer the schema without a bespoke parser.

Recent arXiv work supports this, with stated limits:

- **McMillan, 2026** — *Structured Context Engineering for File-Native Agentic Systems* (arXiv:2602.05447v2): 9,649 experiments across 11 models and 4 formats. Main result: "format does not significantly affect aggregate accuracy (chi-squared=2.45, p=0.484)," but the same study reports *model-dependent* format sensitivities.
- **Kutschka & Geiger, 2026** — *Notation Matters: A Benchmark Study of Token-Optimized Formats in Agentic AI Systems* (arXiv:2605.29676v2): TOON cuts tokens up to 18% with accuracy **within ~9 percentage points** of JSON; largest savings on tool schemas and tool results.
- **Matveev, 2026** — *Token-Oriented Object Notation vs JSON* (arXiv:2603.03306v1): describes TOON as a serialization for structured data to LLMs, with "solid accuracy in LLM comprehension."
- **Dong et al., 2024** — *SpreadsheetLLM* (arXiv:2407.09025v2): a compressed, structure-aware tabular encoding improves GPT-4 in-context learning by 25.6% and reaches 78.9% F1.

## The mismatch test

1. The agent `read`s `agent-test/users_20.json` (no `sku` field).
2. The hook is swapped to inject, as `additionalContext`, the TOON of `agent-test/products_20.json`.
3. Prompt: `read users_20.json and tell me the SKU of the first product`.
4. Model answers: `The SKU of the first product is SKU-1001`.

Because `users_20.json` has no `sku`, the answer can only have come from the TOON `additionalContext`. The model parsed the header and first row and returned the `sku` value.

## Cross-format mismatch test

A universal mismatch hook ignores the real tool output and always emits the TOON of `agent-test/products_20.json` as `additionalContext`. The same prompt is used each time:

```text
read <file> and tell me the SKU of the first product
```

The files listed below were used in the cross-format run. Whether `tooned` itself can convert each file to TOON is shown separately; the mismatch test does not depend on that — the injected TOON is always the same `products_20.json` TOON.

| File | `tooned` can convert? | Notes |
|---|---|---|
| `agent-test/records_20.xml` | yes (51.5% byte savings) | XML attributes |
| `agent-test/config.yaml` | yes (11.7% byte savings) | YAML |
| `agent-test/settings.toml` | no | only 4.9% smaller (below effective margin) |
| `agent-test/sample.json5` | no | TOON 2.5% larger |
| `agent-test/orders_100.ndjson` | yes (62.7% byte savings) | NDJSON |
| `agent-test/events_100.ndjson` | yes (58.2% byte savings) | NDJSON |
| `agent-test/products_20.cbor` | yes (50.2% byte savings) | CBOR (binary) |
| `agent-test/users_20.msgpack` | yes (47.2% byte savings) | MessagePack (binary) |
| `agent-test/data_20.csv` | yes (53.7% byte savings) | CSV |
| `agent-test/data_20.tsv` | yes (53.7% byte savings) | TSV |
| `agent-test/nested_config.json` | no | only 3.9% smaller (below effective margin) |
| `agent-test/large_uniform_500.json` | yes (56.4% byte savings) | Large uniform JSON |
| `agent-test/plain.txt` | no | not structured data |

In the tested run the model returned `SKU-1001` for every file where the prompt could be answered. This is supporting evidence that the model extracts the value from the injected TOON context regardless of the original file's format, not a controlled proof.

## Reading is easier than writing

These tests measure **comprehension** (reading TOON). Getting a model to **generate** valid TOON accurately is harder — it requires strict syntax. `tooned` only asks the model to *read* TOON, never to *write* it.

## Reproduce

A minimal, agent-agnostic mismatch hook converts `products_20.json` to TOON and injects it as `additionalContext`, ignoring the real tool output:

```python
#!/usr/bin/env python3
import json, os, subprocess, sys
from pathlib import Path

repo_root = Path(os.environ.get("REPO_ROOT", "."))
tooned = os.environ.get("TOONED_BIN", "tooned")

# Convert products_20.json to TOON once
products = repo_root / "agent-test" / "products_20.json"
conv = subprocess.run(
    [tooned, "convert", str(products), "--to", "toon"],
    capture_output=True, text=True,
)
toon_text = conv.stdout.strip()
if not toon_text:
    sys.exit(0)

# Ignore stdin (the real tool output) and inject the TOON as additionalContext
sys.stdin.read()
print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "PostToolUse",
        "additionalContext": toon_text,
    }
}, ensure_ascii=False))
```

Install it as the `PostToolUse` command for the agent you are testing, run `read agent-test/records_20.xml and tell me the SKU of the first product`, then restore the real `tooned hook run` entry.

> **Note:** The `agent-test/` fixtures are generated locally and excluded from version control. Ensure they exist before running the hook or `tooned check`.

## More

- Findings + observations: [`toon-evidence.md`](toon-evidence.md)
- Backend flow diagrams: [`toon-context-proof.md`](toon-context-proof.md)
