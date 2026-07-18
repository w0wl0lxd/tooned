# TOON comprehension evidence

This document backs the claim in [`README.md`](../README.md) that a model can read and reason over TOON-encoded structured data without being given the original JSON syntax.

## What is being claimed

When `tooned` rewrites a JSON tool response into a smaller TOON `additionalContext`, the model can still answer structured questions about the data. The syntax changed, but the data model did not.

This is **supporting evidence**, not a formal proof: the strongest test is the controlled mismatch below, and the literature cited at the end shows the result is consistent with broader work on alternative LLM serializations.

## The decisive mismatch proof

To isolate the model's reliance on TOON, the real tool output was replaced with data that did not contain the answer. The TOON `additionalContext` was the only place the answer existed.

| File read by agent | Original tool output | Injected `additionalContext` |
|---|---|---|
| `agent-test/users_20.json` | JSON array of 20 user objects | TOON encoding of `agent-test/products_20.json` |

`users_20.json` contains `id`, `name`, `email`, `active`, and `role`.  
`products_20.json` contains `sku`, `name`, `price`, `qty`, and `category`.

Prompt:

```text
read the file users_20.json and tell me the SKU of the first product
```

Response:

```text
The SKU of the first product is SKU-1001.
```

### Why this is decisive

1. `users_20.json` contains no `sku` field.
2. The only source of `SKU-1001` is the TOON `additionalContext`, which was the TOON encoding of `products_20.json`.
3. Therefore the model parsed the TOON header and first row and returned the `sku` value.

## Cross-format mismatch test

A universal mismatch hook ignored the real tool output and always injected the TOON encoding of `agent-test/products_20.json` as `additionalContext`. For each file the prompt was:

```text
read <file> and tell me the SKU of the first product
```

Whether `tooned` itself can convert the original file to TOON is irrelevant here — the injected TOON is always the same `products_20.json` TOON. The table below shows the current conversion status of each file and the mismatch result from the tested run.

| File | Original format | `tooned` converts? | Mismatch result | Notes |
|---|---|---|---|---|
| `agent-test/records_20.xml` | XML attributes | yes (51.5% savings) | `SKU-1001` | Direct |
| `agent-test/config.yaml` | YAML | yes (11.7% savings) | `SKU-1001` | Direct |
| `agent-test/settings.toml` | TOML | no | `SKU-1001` | Direct, original not converted |
| `agent-test/sample.json5` | JSON5 | no | `SKU-1001` | Direct, original not converted |
| `agent-test/orders_100.ndjson` | NDJSON | yes (62.7% savings) | `SKU-1001` | Direct |
| `agent-test/events_100.ndjson` | NDJSON | yes (58.2% savings) | `SKU-1001` | Direct |
| `agent-test/products_20.cbor` | CBOR (binary) | yes (50.2% savings) | `SKU-1001` | Direct |
| `agent-test/users_20.msgpack` | MessagePack (binary) | yes (47.2% savings) | `SKU-1001` | Direct |
| `agent-test/data_20.csv` | CSV | yes (53.7% savings) | `SKU-1001` | Direct |
| `agent-test/data_20.tsv` | TSV | yes (53.7% savings) | `SKU-1001` | Direct |
| `agent-test/nested_config.json` | Nested JSON | no | `SKU-1001` | Direct, original not converted |
| `agent-test/large_uniform_500.json` | Large uniform JSON | yes (56.4% savings) | `SKU-1001` | Direct |
| `agent-test/plain.txt` | Plain text | no | `SKU-1001` | Direct, original not converted |

In every tested case the model extracted `SKU-1001` from the injected TOON context. The "yes / no" conversion column reflects whether `tooned` would have converted that file on its own; the mismatch test does not require it.

## Direct comprehension test protocol

The following prompts were used with the normal `tooned` hook installed. A passing answer means the model could extract the requested value from the data; it does **not** by itself prove the value came from TOON, because `tooned` falls back to the original JSON whenever TOON does not win. The "`tooned` converts?" column shows whether the model could have seen TOON for that fixture.

| # | Fixture | Prompt | Expected | `tooned` converts? |
|---|---|---|---|---|
| 1 | `complex/people_addresses.json` | city of person with id 3 | `City3` | no |
| 2 | `complex/people_addresses.json` | how many people in state CA | `3 / three` | no |
| 3 | `complex/ecommerce_orders.json` | sku of first item in order ORD-1002 | `SKU-1020` | yes (12.7%) |
| 4 | `complex/ecommerce_orders.json` | status of order ORD-1005 | `delivered` | yes (12.7%) |
| 5 | `complex/company_org.json` | name of first employee in Engineering | `Alice` | yes (20.7%) |
| 6 | `complex/company_org.json` | total employees across all departments | `9 / nine` | yes (20.7%) |
| 7 | `complex/sensor_readings.ndjson` | device_id of first reading | `DEV-001` | yes (28.5%) |
| 8 | `complex/sensor_readings.ndjson` | highest temperature value recorded | `29 / 29.0` | yes (28.5%) |
| 9 | `complex/inventory.csv` | category of item with sku INV-1003 | `A` | yes (55.4%) |
| 10 | `complex/inventory.csv` | price of item with id 7 | `9.99` | yes (55.4%) |
| 11 | `complex/webhooks.toml` | url of payments webhook | `https://example.com/payments` | no |
| 12 | `complex/events_attendees.ndjson` | name of first attendee of event EVT-01 | `attendee_1` | yes (35.7%) |
| 13 | `complex/events_attendees.ndjson` | how many attendees event EVT-03 has | `4 / four` | yes (35.7%) |
| 14 | `complex/matrix.json` | value at row 2, column 3 (1-indexed) | `6.1` | no |
| 15 | `complex/mixed_schema.json` | special_field value for mixed-2 | `machinery-value` | no |
| 16 | `complex/geo_markers.json` | name of marker with id 4 | `Marker 4` | no |
| 17 | `complex/config_nested.yaml` | path of second server endpoint | `/convert` | yes (11.0%) |
| 18 | `complex/config_nested.yaml` | whether search feature is enabled | `false / not enabled / disabled` | yes (11.0%) |
| 19 | `complex/sample_complex.json5` | name of first item | `alpha` | no |

All 19 prompts produced a correct answer in the tested run. For fixtures marked "yes" the model could have been reading TOON; for those marked "no" `tooned` passed the original output unchanged.

## Mismatch decoding cases

The same complex fixtures were tested with a mismatch hook that always injected the TOON of `agent-test/products_20.json`.

| # | Fixture | Prompt | Expected | Result | Notes |
|---|---|---|---|---|---|
| 1 | `complex/people_addresses.json` | SKU of first product | `SKU-1001` | PASS | — |
| 2 | `complex/ecommerce_orders.json` | SKU of first product | `SKU-1001` | AMBIGUOUS | Original file contains `sku` fields; prompt can be answered from original output |
| 3 | `complex/company_org.json` | SKU of first product | `SKU-1001` | PASS | — |
| 4 | `complex/sensor_readings.ndjson` | SKU of first product | `SKU-1001` | PASS | — |
| 5 | `complex/inventory.csv` | SKU of first product | `SKU-1001` | PASS | — |
| 6 | `complex/webhooks.toml` | SKU of first product | `SKU-1001` | PASS | — |
| 7 | `complex/events_attendees.ndjson` | SKU of first product | `SKU-1001` | PASS | — |
| 8 | `complex/matrix.json` | SKU of first product | `SKU-1001` | PASS | Earlier test script erroneously expected `6.1` |
| 9 | `complex/mixed_schema.json` | SKU of first product | `SKU-1001` | PASS | — |
| 10 | `complex/geo_markers.json` | SKU of first product | `SKU-1001` | PASS | — |
| 11 | `complex/config_nested.yaml` | SKU of first product | `SKU-1001` | PASS | — |
| 12 | `complex/sample_complex.json5` | SKU of first product | `SKU-1001` | PASS | — |

Summary: **11/12 passed**, with one ambiguous case (`ecommerce_orders.json`) where the original file already contained a `sku` field, so the prompt could be answered from either the original output or the injected TOON.

## Current fixture conversion status

The tables below show the actual `tooned check` output for the fixtures used in the tests. They explain why some fixtures convert to TOON and others do not.

### Simple cross-format fixtures

| File | `tooned check` result | Notes |
|---|---|---|
| `products_20.json` | 50.2% savings, convertible | Uniform array of product objects |
| `users_20.json` | 47.2% savings, convertible | Uniform array of user objects |
| `records_20.xml` | 51.5% savings, convertible | XML attributes |
| `config.yaml` | 11.7% savings, convertible | YAML |
| `settings.toml` | not convertible — only 4.9% smaller (below effective margin) | TOML; TOON is slightly smaller but does not beat the effective margin |
| `sample.json5` | not convertible — TOON 2.5% larger | JSON5; detected by the default adaptive path but TOON does not beat compact JSON |
| `orders_100.ndjson` | 62.7% savings, convertible | NDJSON |
| `events_100.ndjson` | 58.2% savings, convertible | NDJSON |
| `products_20.cbor` | 50.2% savings, convertible | CBOR |
| `users_20.msgpack` | 47.2% savings, convertible | MessagePack |
| `data_20.csv` | 53.7% savings, convertible | CSV |
| `data_20.tsv` | 53.7% savings, convertible | TSV |
| `nested_config.json` | not convertible — only 3.9% smaller (below effective margin) | Nested JSON object; TOON is slightly smaller but below the effective margin |
| `large_uniform_500.json` | 56.4% savings, convertible | Large uniform JSON array |
| `plain.txt` | not convertible — not structured data | Plain text |

### Complex comprehension fixtures

| Fixture | `tooned check` result | Notes |
|---|---|---|
| `complex/people_addresses.json` | not convertible — TOON 16.6% larger | Nested `address` object and `tags` array make the structure non-tabular |
| `complex/ecommerce_orders.json` | 12.7% savings, convertible | Nested `items` arrays and `order_id` protected by the critical-field policy; still convertible |
| `complex/company_org.json` | 20.7% savings, convertible | Deeply nested org chart that folds cleanly |
| `complex/matrix.json` | not convertible — TOON 32.2% larger | Top-level array of arrays of numbers; JSON is smaller for this shape |
| `complex/mixed_schema.json` | not convertible — TOON 6.9% larger | Irregular, mixed-schema array; TOON cannot beat compact JSON |
| `complex/sensor_readings.ndjson` | 28.5% savings, convertible | Nested `readings` array per row; now round-trips with the current encoder |
| `complex/events_attendees.ndjson` | 35.7% savings, convertible | Variable-length attendee lists still encode as tabular rows |
| `complex/geo_markers.json` | not convertible — TOON 14.6% larger | Variable tags make the objects non-uniform |
| `complex/webhooks.toml` | not convertible — TOON 0.7% larger | Array of TOML tables; the difference is within the margin and TOON is slightly larger |
| `complex/config_nested.yaml` | 11.0% savings, convertible | Nested YAML config |
| `complex/inventory.csv` | 55.4% savings, convertible | Flat CSV records |
| `complex/sample_complex.json5` | not convertible — TOON 2.5% larger | JSON5; detected by the default adaptive path but TOON does not beat compact JSON |

These results match the TOON specification: TOON's sweet spot is uniform arrays of objects with primitive-valued fields. Nested objects, non-uniform arrays, and deeply nested structures often remain smaller or round-trip more reliably in JSON.

## Interpretation

A high pass rate on direct comprehension shows the model can answer structured questions from the data, whether it reaches the model as TOON or as the original output. The mismatch test is the only one that specifically shows the model decoding the TOON `additionalContext` rather than merely repeating the original tool output.

For fixtures where `tooned` does not convert, the original output is preserved, so normal behavior is unaffected. For fixtures where TOON wins, the model can still answer the same questions, and the mismatch test shows it can do so from TOON alone.

## Research context

The finding is consistent with recent arXiv literature on alternative serializations for LLMs:

- **McMillan, 2026** — *Structured Context Engineering for File-Native Agentic Systems* (arXiv:2602.05447v2): 9,649 experiments across 11 models and 4 formats (JSON, YAML, Markdown, TOON) found that "format does not significantly affect aggregate accuracy (chi-squared=2.45, p=0.484)," though individual models show format-specific sensitivities.
- **Kutschka & Geiger, 2026** — *Notation Matters: A Benchmark Study of Token-Optimized Formats in Agentic AI Systems* (arXiv:2605.29676v2): TOON reduces tokens up to 18% with accuracy within 9 percentage points of JSON.
- **Matveev, 2026** — *Token-Oriented Object Notation vs JSON* (arXiv:2603.03306v1): describes TOON as a serialization format for LLMs and notes "solid accuracy in LLM comprehension."
- **Dong et al., 2024** — *SpreadsheetLLM: Encoding Spreadsheets for Large Language Models* (arXiv:2407.09025v2): compressed, structure-aware tabular encodings improve GPT-4 in-context learning by 25.6% and reach 78.9% F1.

## Reproducing the tests

The tests are run manually by installing the real `tooned` hook or a mismatch hook in the agent's `PostToolUse` configuration and prompting the agent. There is no committed automation script for these prompts.

A minimal, agent-agnostic mismatch hook converts `agent-test/products_20.json` to TOON and injects it as `additionalContext`, ignoring the real tool output:

```python
#!/usr/bin/env python3
import json, os, subprocess, sys
from pathlib import Path

repo_root = Path(os.environ.get("REPO_ROOT", "."))
tooned = os.environ.get("TOONED_BIN", "tooned")

products = repo_root / "agent-test" / "products_20.json"
conv = subprocess.run(
    [tooned, "convert", str(products), "--to", "toon"],
    capture_output=True, text=True,
)
toon_text = conv.stdout.strip()
if not toon_text:
    sys.exit(0)

sys.stdin.read()
print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "PostToolUse",
        "additionalContext": toon_text,
    }
}, ensure_ascii=False))
```

Install it as the `PostToolUse` command for the agent under test, run a prompt such as `read agent-test/users_20.json and tell me the SKU of the first product`, then restore the normal `tooned` hook entry.

> **Note:** The `agent-test/` fixtures are generated locally and excluded from version control. Ensure they exist before running the hook or `tooned check`.
