# TOON Decoding Test Suite

This document records the cross-format test protocol and results for whether a Large Language Model, presented with a TOON-encoded tool result from a `PostToolUse` hook or wrapped command, can decode the TOON internally and answer questions correctly.

> **Historical note.** Early versions of this test suite injected TOON as `additionalContext`. That approach is flawed: `additionalContext` keeps the original JSON in the model's context and appends the TOON, inflating total token count rather than reducing it. The current protocol uses tool-result replacement (`updatedToolOutput` for Claude Code/OpenCode/Kilo/Pi, `continue: false` + `reason` feedback for Codex) so the model sees *only* the TOON. For Devin and Droid, which only support `additionalContext` in `PostToolUse`, use command-level wrapping (`tooned wrap -- <cmd>` or `... | tooned pipe`) to deliver TOON-only output.

## Methodology

Fixtures live under `agent-test/` and `agent-test/complex/`. Each fixture is tested with a live agent prompt while the real `tooned` hook or a mismatch hook is installed.

- **Direct**: the normal `tooned hook run` converts the file to TOON (when it wins) and replaces the tool result; the original JSON is no longer in that context item.
- **Mismatch**: a temporary hook replaces every tool response with the TOON encoding of `agent-test/products_20.json` and the prompt asks for the SKU of the first product. Because the original file does not contain `sku`, a correct `SKU-1001` answer must come from the replacement TOON result.

A response is marked **PASS** if it contains one of the expected strings (case-insensitive). A response is marked **AMBIGUOUS** if the original file already contains a field that could answer the prompt.

## Results summary

- **Direct comprehension**: 19/19 passed
- **Mismatch decoding**: 11/12 passed, 1 ambiguous
- **Total test cases**: 31

## Direct comprehension results

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

All 19 direct prompts produced a correct answer in the tested run. For fixtures where `tooned` converts, the tool result was replaced with TOON, so the model saw only TOON; for fixtures where `tooned` does not convert, the answer came from the original tool output.

## Mismatch decoding results

| # | Fixture | Expected | Result | Notes |
|---|---|---|---|---|
| 1 | `complex/people_addresses.json` | `SKU-1001` | PASS | — |
| 2 | `complex/ecommerce_orders.json` | `SKU-1001` | AMBIGUOUS | Original file contains `sku` fields; prompt can be answered from original output |
| 3 | `complex/company_org.json` | `SKU-1001` | PASS | — |
| 4 | `complex/sensor_readings.ndjson` | `SKU-1001` | PASS | — |
| 5 | `complex/inventory.csv` | `SKU-1001` | PASS | — |
| 6 | `complex/webhooks.toml` | `SKU-1001` | PASS | — |
| 7 | `complex/events_attendees.ndjson` | `SKU-1001` | PASS | — |
| 8 | `complex/matrix.json` | `SKU-1001` | PASS | Earlier test script erroneously expected `6.1` |
| 9 | `complex/mixed_schema.json` | `SKU-1001` | PASS | — |
| 10 | `complex/geo_markers.json` | `SKU-1001` | PASS | — |
| 11 | `complex/config_nested.yaml` | `SKU-1001` | PASS | — |
| 12 | `complex/sample_complex.json5` | `SKU-1001` | PASS | — |

## Interpretation

A high pass rate on direct comprehension shows the model can answer structured questions from the data, whether it reaches the model as TOON or as the original JSON. A high pass rate on mismatch tests shows the model specifically decodes the TOON tool result rather than merely repeating the original tool output.

The one ambiguous mismatch case (`ecommerce_orders.json`) is a test-design issue: the original file already contains `sku` values, so the prompt is not a clean isolation of the TOON result. Asking for a field absent from the original file, such as the product `name` (`Product 1`), would make the test unambiguous.

For agents that only support `additionalContext` in `PostToolUse` (Devin, Droid), `tooned` does not emit `additionalContext`; use `tooned wrap -- <cmd>` or `... | tooned pipe` to deliver TOON-only output with those agents.

## Research context

This finding is consistent with recent arXiv literature:

- **McMillan, 2026** — *Structured Context Engineering for File-Native Agentic Systems* (arXiv:2602.05447v2): 9,649 experiments across 11 models and 4 formats (JSON, YAML, Markdown, TOON) found that "format does not significantly affect aggregate accuracy (chi-squared=2.45, p=0.484)."
- **Kutschka & Geiger, 2026** — *Notation Matters: A Benchmark Study of Token-Optimized Formats in Agentic AI Systems* (arXiv:2605.29676v2): TOON reduces tokens up to 18% with accuracy within 9 percentage points of JSON.
- **Matveev, 2026** — *Token-Oriented Object Notation vs JSON* (arXiv:2603.03306v1): describes TOON as a serialization format for LLMs and notes "solid accuracy in LLM comprehension."
- **Dong et al., 2024** — *SpreadsheetLLM* (arXiv:2407.09025v2): compressed, structure-aware tabular encodings improve GPT-4 in-context learning by 25.6% and reach 78.9% F1.

## Reproduction

The tests are run manually. A minimal mismatch hook for an agent that supports tool-result replacement (Claude Code / OpenCode / Kilo / Pi) converts `agent-test/products_20.json` to TOON and replaces the tool output:

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
        "updatedToolOutput": toon_text,
    }
}, ensure_ascii=False))
```

For Codex, use `{"continue": false, "reason": toon_text, "hookSpecificOutput": {"hookEventName": "PostToolUse"}}` instead. For Devin / Droid, `PostToolUse` cannot replace the tool result, so use command-level wrapping (`tooned wrap -- cat agent-test/products_20.json`) and prompt the agent to read the wrapped output.

Install the appropriate hook as the `PostToolUse` command for the agent under test and run prompts such as:

```text
read agent-test/complex/company_org.json and tell me the SKU of the first product
```

Restore the real `tooned hook run` entry afterwards.

> **Note:** The `agent-test/` fixtures are generated locally and excluded from version control. Ensure they exist before running the hook or `tooned check`.

For details on why some fixtures do not convert, see [`toon-format-research.md`](toon-format-research.md).
