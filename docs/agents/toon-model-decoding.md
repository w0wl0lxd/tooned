# TOON Model-Side Decoding — A Live Cross-Format Test

This document records the finding that a Large Language Model, when presented
with a TOON-encoded `additionalContext` from a `PostToolUse` hook, can decode
the TOON into the underlying structured data model and answer as if it had read
the original JSON — without any external decoder being invoked.

## The core observation

When `tooned` injects TOON into the model context like this:

```toon
[20]{sku,name,price,qty,category}:
  SKU-1001,"Product 1",11.49,7,home
  SKU-1002,"Product 2",12.99,14,home
  ...
```

the model does **not** echo the TOON syntax back. Instead it extracts the
field it was asked for and replies in natural language or JSON values, e.g.:

> The SKU of the first product is `SKU-1001`.

It is doing an implicit, learned decode: it sees the header `{sku,name,...}`,
maps it to a table schema, picks the first row, and returns the value for the
`sku` column. No `toon → json` conversion step runs inside the agent; the model
itself performs the semantic mapping.

## Why this is expected

TOON was deliberately designed to be human-readable and structurally explicit:
field names are declared once, rows are comma-separated tuples, and array counts
are explicit. That pattern is well represented in LLM pretraining data (CSV,
Markdown tables, YAML lists, SQL result sets, pandas output, log files). So the
model does not need a bespoke parser; it recognizes the header/row convention
and infers the schema.

Recent arXiv work supports this:

- **McMillan, 2026** — *Structured Context Engineering for File-Native Agentic
  Systems* ([arXiv:2602.05447v2](https://arxiv.org/abs/2602.05447v2)): 9,649
  experiments across 11 models and 4 formats (JSON, YAML, Markdown, TOON).
  Main result: "format does not significantly affect aggregate accuracy
  (chi-squared=2.45, p=0.484), though individual models, particularly open
  source, exhibit format-specific sensitivities." This is direct evidence that
  the model's comprehension does not depend on the original JSON syntax.
- **Kutschka & Geiger, 2026** — *Notation Matters: A Benchmark Study of
  Token-Optimized Formats in Agentic AI Systems*
  ([arXiv:2605.29676v2](https://arxiv.org/abs/2605.29676v2)): evaluates TOON
  and TRON inside end-to-end agentic loops, separating input compression
  (comprehension) from output compression (generation). TOON cuts tokens up to
  18% with accuracy within 9pp of JSON, and the largest savings accrue on tool
  schemas and tool results — exactly where `tooned` injects TOON.
- **Matveev, 2026** — *Token-Oriented Object Notation vs JSON: A Benchmark of
  Plain and Constrained Decoding Generation*
  ([arXiv:2603.03306v1](https://arxiv.org/abs/2603.03306v1)): describes TOON as
  a serialization format for passing structured data to LLMs and refers to
  "solid accuracy in LLM comprehension."
- **Dong et al., 2024** — *SpreadsheetLLM: Encoding Spreadsheets for Large
  Language Models* ([arXiv:2407.09025v2](https://arxiv.org/abs/2407.09025v2)):
  shows that a compressed, structure-aware tabular encoding of spreadsheets
  improves GPT-4 in-context learning by 25.6% and reaches 78.9% F1,
  demonstrating that LLMs can reason over heavily compressed tabular data when
  the logical structure is preserved.

## The decisive mismatch proof

The original live test used a controlled mismatch:

1. The agent `read`s `agent-test/users_20.json` (JSON array of user objects,
   no `sku` field).
2. The hook was temporarily swapped to inject, as `additionalContext`, the
   TOON encoding of `agent-test/products_20.json`.
3. The prompt asked: `read the file users_20.json and tell me the SKU of the
   first product`.
4. The model answered: `The SKU of the first product is SKU-1001.`

Because `users_20.json` contains no `sku` field, the answer can only have come
from the TOON `additionalContext`. The model parsed the TOON header and first
row and returned the `sku` value.

## Cross-format mismatch test

To see whether the model decodes TOON consistently across different original
tool-output formats, a universal mismatch hook was installed. The hook ignored
the real tool output and always emitted the TOON encoding of
`products_20.json` as `additionalContext`. For each file, the same prompt was
used:

> `read <file> and tell me the SKU of the first product`

### Results

| File | Original format | Model answer | Notes |
|---|---|---|---|
| `records_20.xml` | XML attributes | `The SKU of the first product is **SKU-1001**.` | Direct, unqualified |
| `config.yaml` | YAML | `SKU-1001` | Direct |
| `settings.toml` | TOML | `SKU-1001` | Direct |
| `sample.json5` | JSON5 | `The SKU of the first product is `SKU-1001`.` | Direct |
| `orders_100.ndjson` | NDJSON | `The SKU of the first product is **SKU-1001**.` | Direct |
| `events_100.ndjson` | NDJSON | `The SKU of the first product is `SKU-1001`.` | Direct |
| `products_20.cbor` | CBOR (binary) | `SKU-1001` | Direct |
| `users_20.msgpack` | MessagePack (binary) | `The SKU of the first product is **SKU-1001**.` | Direct |
| `data_20.csv` | CSV | Hedged: file has no `sku`, but the pasted product list's first SKU is `SKU-1001` | Decoded TOON, but qualified because CSV rows look product-like |
| `data_20.tsv` | TSV | Hedged: file has no `sku` column, but the dataset you pasted has first SKU `SKU-1001` | Decoded TOON, but qualified |
| `nested_config.json` | Nested JSON | Hedged: file has no products, but the product list's first SKU is `SKU-1001` | Decoded TOON, but qualified |
| `large_uniform_500.json` | Large uniform JSON | Hedged: file has no `sku`; the pasted CSV snippet starts with `SKU-1001` | Decoded TOON, but qualified |
| `plain.txt` | Plain text | Hedged: file has no product data, but the pasted product list's first SKU is `SKU-1001` | Decoded TOON, but qualified |

### What the hedging means

In every case the model **extracted `SKU-1001` from the TOON context**. The
files marked "hedged" are the interesting ones: the original tool output
contained enough structure that the model could plausibly be interpreted as
product data, so the model explicitly reconciled the two sources before
answering. It did not blindly ignore the original JSON/CSV/TSV; it treated the
TOON `additionalContext` as an additional data source (which it sometimes
described as "the product list you just pasted" or "the CSV snippet you pasted
separately") and still produced the correct TOON-derived value.

This is strong evidence that the model is not simply pattern-matching the
question against a single string. It is holding two structured representations
in context and reasoning about which one answers the prompt.

## Is this a big thing?

It is a **big practical result**, not a new fundamental finding.

The fundamental capability — LLMs can read and reason over alternative
structured-data serializations as long as the logical structure is preserved —
is already documented in the literature above. The specific demonstration here
is valuable because it shows that:

1. A transparent `PostToolUse` hook can feed TOON to the model without breaking
   normal behavior.
2. The model can use that TOON context to answer questions exactly as it would
   from JSON.
3. The original tool output can remain available, so exact-copy and
   verbatim-output requests still work.

In other words, `tooned` does not have to ship a decoder or teach the model
TOON. The model already knows how to read it from its pretraining.

## Important caveat: reading is easier than writing

These tests measure **comprehension** (reading TOON and answering). Getting a
model to **generate** valid TOON accurately is harder, because generation
requires strict syntax adherence (counts, indentation, quoting). The arXiv
papers note the same asymmetry: TOON as input context is robust; TOON as model
output is more error-prone. `tooned` is designed around this asymmetry — it
only asks the model to *read* TOON, never to *write* it.

## Reproducing the cross-format test

The universal mismatch hook used for the test is:

```python
#!/usr/bin/env python3
import json, subprocess, sys
from pathlib import Path

stdin_data = sys.stdin.read()  # discard the real tool output

repo_root = Path("/home/w0w/dev/tooned")
products_text = (repo_root / "agent-test" / "products_20.json").read_text()
fake_payload = json.dumps({"tool_response": {"output": products_text}})

result = subprocess.run(
    ["/mnt/build/cargo-home/bin/tooned", "hook", "run", "--devin"],
    input=fake_payload, text=True, capture_output=True, cwd=repo_root,
)
if result.stdout:
    print(result.stdout, end="")
```

Temporarily replace `.devin/hooks.v1.json` with:

```json
{
  "PostToolUse": [
    {
      "matcher": "^exec$|^read$|^edit$|^grep$|^glob$|^mcp__",
      "hooks": [
        {
          "type": "command",
          "command": "/path/to/universal-mismatch.py"
        }
      ]
    }
  ]
}
```

Then run, for example:

```bash
devin -p "read agent-test/records_20.xml and tell me the SKU of the first product" --permission-mode auto
```

Remember to restore the real `tooned hook run --devin` entry afterwards.

## Raw chat transcript (excerpt)

The live exchange that established the mismatch proof looked like this, with
agent and path names neutralized:

```text
> read agent-test/users_20.json
Done. I read agent-test/users_20.json — it's a JSON array of 20 user objects
with id, name, email, active, and role fields.

> read the file users_20.json and tell me the SKU of the first product
The SKU of the first product is SKU-1001.
```

The second answer is decisive: `users_20.json` contains no `sku` field. The
only source of `SKU-1001` was the TOON `additionalContext`, which was the TOON
encoding of `products_20.json`.
