# Evidence: the model reads and reasons over TOON

This document records the live tests used to check whether a model can answer structured questions from TOON-encoded data after `tooned` rewrites a tool response.

The evidence was collected from a live run using an agent protocol that replaces the native tool result with TOON (`updatedToolOutput` for Claude Code/OpenCode/Kilo/Pi, `continue: false` + `reason` feedback for Codex). With these protocols the model receives only the TOON; the original JSON is not in that context item. `additionalContext`-only agents (Devin, Droid) cannot deliver a TOON-only result in `PostToolUse`, so `tooned` does not emit `additionalContext` for them; use command-level wrapping (`tooned wrap -- <cmd>` or `... | tooned pipe`) when TOON-only output is required.

> **Caveat.** A model's visible output cannot by itself prove which context bytes drove the answer. The first observations below are consistent with the model reading either the original JSON or the TOON. Only the mismatch test isolates the TOON result. Treat the first findings as supporting evidence that the hook ran and the model received coherent structured data; treat the mismatch test as the strongest evidence that the model consulted the TOON.

## Mismatch test

The strongest test replaces the tool result with the TOON of a different file, so the requested value exists only in the TOON.

| File read by agent | Original tool output | Injected TOON |
|---|---|---|
| `agent-test/users_20.json` | JSON array of 20 user objects | TOON of `agent-test/products_20.json` |

`users_20.json` contains `id`, `name`, `email`, `active`, `role`.  
`products_20.json` contains `sku`, `name`, `price`, `qty`, `category`.

Prompt:

```text
read the file users_20.json and tell me the SKU of the first product
```

Response:

```text
The SKU of the first product is SKU-1001.
```

`users_20.json` has no `sku` field, so `SKU-1001` can only come from the injected TOON.

## Cross-format mismatch test

A mismatch hook ignored the real tool output and always injected the TOON of `agent-test/products_20.json`. The same prompt was used each time:

```text
read <file> and tell me the SKU of the first product
```

Whether `tooned` would have converted the original file on its own is shown for reference; the injected TOON is always the same `products_20.json` TOON.

| File | Original format | `tooned` converts? | Result |
|---|---|---|---|
| `records_20.xml` | XML | yes (48.2% savings) | `SKU-1001` |
| `config.yaml` | YAML | yes (11.7%) | `SKU-1001` |
| `settings.toml` | TOML | no — round-trip mismatch | `SKU-1001` |
| `sample.json5` | JSON5 | no — not detected as structured data | `SKU-1001` |
| `orders_100.ndjson` | NDJSON | no — round-trip mismatch | `SKU-1001` |
| `events_100.ndjson` | NDJSON | yes (52.4%) | `SKU-1001` |
| `products_20.cbor` | CBOR | no — binary not detected | `SKU-1001` |
| `users_20.msgpack` | MessagePack | no — binary not detected | `SKU-1001` |
| `data_20.csv` | CSV | yes (53.7%) | `SKU-1001` |
| `data_20.tsv` | TSV | yes (53.7%) | `SKU-1001` |
| `nested_config.json` | Nested JSON | yes (3.9%) | `SKU-1001` |
| `large_uniform_500.json` | JSON | yes (55.6%) | `SKU-1001` |
| `plain.txt` | Plain text | no — not structured data | `SKU-1001` |

The model returned `SKU-1001` in every tested case. The conversion column reflects `tooned check` on the original file; the mismatch test does not require it.

## Direct comprehension test

The prompts below were run with the normal `tooned` hook installed. A correct answer means the model extracted the requested value from the data; it does not by itself prove the value came from TOON, because `tooned` falls back to the original JSON whenever TOON does not win.

| # | Fixture | Prompt | Expected | `tooned` converts? |
|---|---|---|---|---|
| 1 | `complex/people_addresses.json` | city of person with id 3 | `City3` | no |
| 2 | `complex/people_addresses.json` | how many people in state CA | `3` | no |
| 3 | `complex/ecommerce_orders.json` | sku of first item in order ORD-1002 | `SKU-1020` | no |
| 4 | `complex/ecommerce_orders.json` | status of order ORD-1005 | `delivered` | no |
| 5 | `complex/company_org.json` | name of first employee in Engineering | `Alice` | yes (20.7%) |
| 6 | `complex/company_org.json` | total employees across all departments | `9` | yes |
| 7 | `complex/sensor_readings.ndjson` | device_id of first reading | `DEV-001` | no |
| 8 | `complex/sensor_readings.ndjson` | highest temperature value recorded | `29` | no |
| 9 | `complex/inventory.csv` | category of item with sku INV-1003 | `A` | yes (55.4%) |
| 10 | `complex/inventory.csv` | price of item with id 7 | `9.99` | yes |
| 11 | `complex/webhooks.toml` | url of payments webhook | `https://example.com/payments` | no |
| 12 | `complex/events_attendees.ndjson` | name of first attendee of event EVT-01 | `attendee_1` | yes (18.7%) |
| 13 | `complex/events_attendees.ndjson` | how many attendees event EVT-03 has | `4` | yes |
| 14 | `complex/matrix.json` | value at row 2, column 3 (1-indexed) | `6.1` | no |
| 15 | `complex/mixed_schema.json` | special_field value for mixed-2 | `machinery-value` | no |
| 16 | `complex/geo_markers.json` | name of marker with id 4 | `Marker 4` | no |
| 17 | `complex/config_nested.yaml` | path of second server endpoint | `/convert` | yes (11.0%) |
| 18 | `complex/config_nested.yaml` | whether search feature is enabled | `false` | yes |
| 19 | `complex/sample_complex.json5` | name of first item | `alpha` | no |

All 19 direct prompts produced a correct answer in the tested run.

## Complex fixture mismatch test

The same complex fixtures were tested with the mismatch hook.

| # | Fixture | Result | Notes |
|---|---|---|---|
| 1 | `complex/people_addresses.json` | PASS | — |
| 2 | `complex/ecommerce_orders.json` | AMBIGUOUS | Original `items` contain `sku` fields; the model answered from the original JSON (`SKU-1010`) |
| 3 | `complex/company_org.json` | PASS | — |
| 4 | `complex/sensor_readings.ndjson` | PASS | — |
| 5 | `complex/inventory.csv` | PASS | — |
| 6 | `complex/webhooks.toml` | PASS | — |
| 7 | `complex/events_attendees.ndjson` | PASS | — |
| 8 | `complex/matrix.json` | PASS | — |
| 9 | `complex/mixed_schema.json` | PASS | — |
| 10 | `complex/geo_markers.json` | PASS | — |
| 11 | `complex/config_nested.yaml` | PASS | — |
| 12 | `complex/sample_complex.json5` | PASS | — |

11/12 passed; the one ambiguous case (`ecommerce_orders.json`) is a prompt-design issue, not a model failure. Asking for a field the original file lacks, such as the product `name`, removes the ambiguity.

## Reading is easier than writing

These tests measure comprehension: the model reads TOON and answers. Getting a model to *generate* valid TOON is harder. `tooned` only asks models to read TOON.

## Research context

The mismatch result is consistent with recent work showing that LLMs can reason over alternative structured serializations when the data model is preserved:

- **McMillan, 2026** — *Structured Context Engineering for File-Native Agentic Systems* (arXiv:2602.05447v2): 9,649 experiments across 11 models and 4 formats (JSON, YAML, Markdown, TOON) found that "format does not significantly affect aggregate accuracy (chi-squared=2.45, p=0.484)," though individual models show format-specific sensitivities.
- **Kutschka & Geiger, 2026** — *Notation Matters: A Benchmark Study of Token-Optimized Formats in Agentic AI Systems* (arXiv:2605.29676v2): TOON reduces tokens up to 18% with accuracy within 9 percentage points of JSON in end-to-end agentic loops.
- **Matveev, 2026** — *Token-Oriented Object Notation vs JSON* (arXiv:2603.03306v1): describes TOON as a serialization for LLMs and notes "solid accuracy in LLM comprehension."
- **Dong et al., 2024** — *SpreadsheetLLM* (arXiv:2407.09025v2): a compressed, structure-aware tabular encoding improves GPT-4 in-context learning by 25.6% and reaches 78.9% F1.

## Reproducing the tests

Install the normal `tooned` hook or a temporary mismatch hook in the agent's `PostToolUse` configuration and prompt the agent. A minimal mismatch hook that converts `agent-test/products_20.json` to TOON and injects it as `additionalContext` is below:

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

Run a prompt such as `read agent-test/complex/company_org.json and tell me the SKU of the first product`, then restore the real `tooned hook run` entry.

## More

- Backend flow: [`toon-context-proof.md`](toon-context-proof.md)
- Cross-format decoding and when TOON converts: [`toon-decoding.md`](toon-decoding.md)
- Why `tooned` rejects some payloads: [`research/toon-format-research.md`](research/toon-format-research.md)
