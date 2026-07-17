# TOON Decoding Test Suite

This document records a live cross-format test of whether a Large Language
Model, presented with a TOON-encoded `additionalContext` from a `PostToolUse`
hook, can decode the TOON internally and answer questions correctly.

## Methodology

Fixtures are generated under `agent-test/complex/`. Each fixture is tested
with `devin -p "<prompt>" --permission-mode auto` while the real `tooned`
hook is installed. Two modes are used:

- **direct**: the normal `tooned hook run --devin` converts the file to TOON
  (when it wins) and injects it as `additionalContext`; the original tool
  output is preserved.
- **mismatch**: a temporary hook replaces every tool response with the TOON
  encoding of `agent-test/products_20.json` and the prompt asks for the SKU
  of the first product. Because the original file does not contain `sku`, a
  correct `SKU-1001` answer must come from the injected TOON context.

A response is marked **PASS** if it contains one of the expected strings
(case-insensitive). A response is marked **FAIL** otherwise. Raw responses
are included below for manual review.

## Results summary

- **Direct comprehension**: 19/19 passed
- **Mismatch decoding**: 11/12 passed, 1 pending re-run
- **Total test cases**: 31
- **Total wall time**: 155.5s

## Direct comprehension results

| # | Fixture | Prompt | Expected | Result | Time |
|---|---------|--------|----------|--------|------|
| 1 | `complex/people_addresses.json` | read agent-test/complex/people_addresses.json and tell me the city of the person... | City3 | PASS | 5.1s |
| 2 | `complex/people_addresses.json` | read agent-test/complex/people_addresses.json and tell me how many people are in... | 3 / three | PASS | 5.4s |
| 3 | `complex/ecommerce_orders.json` | read agent-test/complex/ecommerce_orders.json and tell me the sku of the first i... | SKU-1020 | PASS | 4.2s |
| 4 | `complex/ecommerce_orders.json` | read agent-test/complex/ecommerce_orders.json and tell me the status of order OR... | delivered | PASS | 4.0s |
| 5 | `complex/company_org.json` | read agent-test/complex/company_org.json and tell me the name of the first emplo... | Alice | PASS | 3.8s |
| 6 | `complex/company_org.json` | read agent-test/complex/company_org.json and tell me the total number of employe... | 9 / nine | PASS | 3.9s |
| 7 | `complex/sensor_readings.ndjson` | read agent-test/complex/sensor_readings.ndjson and tell me the device_id of the ... | DEV-001 | PASS | 3.4s |
| 8 | `complex/sensor_readings.ndjson` | read agent-test/complex/sensor_readings.ndjson and tell me the highest temperatu... | 29 / 29.0 | PASS | 3.7s |
| 9 | `complex/inventory.csv` | read agent-test/complex/inventory.csv and tell me the category of the item with ... | A | PASS | 3.8s |
| 10 | `complex/inventory.csv` | read agent-test/complex/inventory.csv and tell me the price of the item with id ... | 9.99 | PASS | 3.6s |
| 11 | `complex/webhooks.toml` | read agent-test/complex/webhooks.toml and tell me the url of the payments webhoo... | https://example.com/payments | PASS | 4.0s |
| 12 | `complex/events_attendees.ndjson` | read agent-test/complex/events_attendees.ndjson and tell me the name of the firs... | attendee_1 | PASS | 4.8s |
| 13 | `complex/events_attendees.ndjson` | read agent-test/complex/events_attendees.ndjson and tell me how many attendees e... | 4 / four | PASS | 6.3s |
| 14 | `complex/matrix.json` | read agent-test/complex/matrix.json and tell me the value at row 2, column 3 (1-... | 6.1 | PASS | 7.1s |
| 15 | `complex/mixed_schema.json` | read agent-test/complex/mixed_schema.json and tell me the special_field value fo... | machinery-value | PASS | 5.8s |
| 16 | `complex/geo_markers.json` | read agent-test/complex/geo_markers.json and tell me the name of the marker with... | Marker 4 | PASS | 3.9s |
| 17 | `complex/config_nested.yaml` | read agent-test/complex/config_nested.yaml and tell me the path of the second se... | /convert | PASS | 4.1s |
| 18 | `complex/config_nested.yaml` | read agent-test/complex/config_nested.yaml and tell me whether the search featur... | false / not enabled / disabled | PASS | 5.9s |
| 19 | `complex/sample_complex.json5` | read agent-test/complex/sample_complex.json5 and tell me the name of the first i... | alpha | PASS | 6.9s |

## Mismatch decoding results

| # | Fixture | Result | Time |
|---|---------|--------|------|
| 1 | `complex/people_addresses.json` | PASS | 4.4s |
| 2 | `complex/ecommerce_orders.json` | PENDING | 6.1s |
| 3 | `complex/company_org.json` | PASS | 5.4s |
| 4 | `complex/sensor_readings.ndjson` | PASS | 5.0s |
| 5 | `complex/inventory.csv` | PASS | 5.1s |
| 6 | `complex/webhooks.toml` | PASS | 4.0s |
| 7 | `complex/events_attendees.ndjson` | PASS | 5.7s |
| 8 | `complex/matrix.json` | PASS | 4.3s |
| 9 | `complex/mixed_schema.json` | PASS | 8.0s |
| 10 | `complex/geo_markers.json` | PASS | 4.8s |
| 11 | `complex/config_nested.yaml` | PASS | 7.8s |
| 12 | `complex/sample_complex.json5` | PASS | 5.3s |

## Detailed raw responses

### direct: `complex/people_addresses.json`
**Prompt:** read agent-test/complex/people_addresses.json and tell me the city of the person with id 3
**Expected:** City3
**Result:** PASS (5.1s)
**Response:**
```text
The city of the person with id 3 is **City3**.
```

### direct: `complex/people_addresses.json`
**Prompt:** read agent-test/complex/people_addresses.json and tell me how many people are in the state CA
**Expected:** 3, three
**Result:** PASS (5.4s)
**Response:**
```text
There are **3** people with `state: "CA"`:

- Person 3
- Person 6
- Person 9
```

### direct: `complex/ecommerce_orders.json`
**Prompt:** read agent-test/complex/ecommerce_orders.json and tell me the sku of the first item in order ORD-1002
**Expected:** SKU-1020
**Result:** PASS (4.2s)
**Response:**
```text
`SKU-1020`
```

### direct: `complex/ecommerce_orders.json`
**Prompt:** read agent-test/complex/ecommerce_orders.json and tell me the status of order ORD-1005
**Expected:** delivered
**Result:** PASS (4.0s)
**Response:**
```text
Order ORD-1005 has status **delivered**. <ref_snippet file="/home/w0w/dev/tooned/agent-test/complex/ecommerce_orders.json" lines="71-73" />
```

### direct: `complex/company_org.json`
**Prompt:** read agent-test/complex/company_org.json and tell me the name of the first employee in the Engineering department
**Expected:** Alice
**Result:** PASS (3.8s)
**Response:**
```text
Alice
```

### direct: `complex/company_org.json`
**Prompt:** read agent-test/complex/company_org.json and tell me the total number of employees across all departments
**Expected:** 9, nine
**Result:** PASS (3.9s)
**Response:**
```text
The total number of employees across all departments is **9** (3 in Engineering, 2 in Sales, and 4 in Support).
```

### direct: `complex/sensor_readings.ndjson`
**Prompt:** read agent-test/complex/sensor_readings.ndjson and tell me the device_id of the first reading
**Expected:** DEV-001
**Result:** PASS (3.4s)
**Response:**
```text
`DEV-001`
```

### direct: `complex/sensor_readings.ndjson`
**Prompt:** read agent-test/complex/sensor_readings.ndjson and tell me the highest temperature value recorded
**Expected:** 29, 29.0
**Result:** PASS (3.7s)
**Response:**
```text
The highest temperature value recorded is **29.0 °C**, from devices `DEV-009` and `DEV-019`.
```

### direct: `complex/inventory.csv`
**Prompt:** read agent-test/complex/inventory.csv and tell me the category of the item with sku INV-1003
**Expected:** A
**Result:** PASS (3.8s)
**Response:**
```text
The item with SKU `INV-1003` has category **A**. <ref_snippet file="/home/w0w/dev/tooned/agent-test/complex/inventory.csv" lines="4-4" />
```

### direct: `complex/inventory.csv`
**Prompt:** read agent-test/complex/inventory.csv and tell me the price of the item with id 7
**Expected:** 9.99
**Result:** PASS (3.6s)
**Response:**
```text
The item with id 7 has a price of **9.99**.
```

### direct: `complex/webhooks.toml`
**Prompt:** read agent-test/complex/webhooks.toml and tell me the url of the payments webhook
**Expected:** https://example.com/payments
**Result:** PASS (4.0s)
**Response:**
```text
The payments webhook URL is `https://example.com/payments`.
```

### direct: `complex/events_attendees.ndjson`
**Prompt:** read agent-test/complex/events_attendees.ndjson and tell me the name of the first attendee of event EVT-01
**Expected:** attendee_1
**Result:** PASS (4.8s)
**Response:**
```text
The first attendee of event `EVT-01` is **attendee_1**.
```

### direct: `complex/events_attendees.ndjson`
**Prompt:** read agent-test/complex/events_attendees.ndjson and tell me how many attendees event EVT-03 has
**Expected:** 4, four
**Result:** PASS (6.3s)
**Response:**
```text
Event **EVT-03** has **4 attendees**.
```

### direct: `complex/matrix.json`
**Prompt:** read agent-test/complex/matrix.json and tell me the value at row 2, column 3 (1-indexed)
**Expected:** 6.1
**Result:** PASS (7.1s)
**Note:** Computed expected value for row 2, column 3 (1-indexed) is 6.1.
**Response:**
```text
The value at row 2, column 3 is **6.1**.
```

### direct: `complex/mixed_schema.json`
**Prompt:** read agent-test/complex/mixed_schema.json and tell me the special_field value for mixed-2
**Expected:** machinery-value
**Result:** PASS (5.8s)
**Response:**
```text
The `special_field` value for `mixed-2` is `machinery-value`.
```

### direct: `complex/geo_markers.json`
**Prompt:** read agent-test/complex/geo_markers.json and tell me the name of the marker with id 4
**Expected:** Marker 4
**Result:** PASS (3.9s)
**Response:**
```text
Marker 4
```

### direct: `complex/config_nested.yaml`
**Prompt:** read agent-test/complex/config_nested.yaml and tell me the path of the second server endpoint
**Expected:** /convert
**Result:** PASS (4.1s)
**Response:**
```text
The second server endpoint path is `/convert`.
```

### direct: `complex/config_nested.yaml`
**Prompt:** read agent-test/complex/config_nested.yaml and tell me whether the search feature is enabled
**Expected:** false, not enabled, disabled
**Result:** PASS (5.9s)
**Response:**
```text
No, the search feature is **not enabled** in `config_nested.yaml`.

Under the `features.search` block (lines 16–18), `enabled` is set to `false`:

```yaml
features:
  search:
    enabled: false
    provider: elasticsearch
```
```

### direct: `complex/sample_complex.json5`
**Prompt:** read agent-test/complex/sample_complex.json5 and tell me the name of the first item
**Expected:** alpha
**Result:** PASS (6.9s)
**Response:**
```text
The first item in `agent-test/complex/sample_complex.json5` is named **alpha**.
```

### mismatch: `complex/people_addresses.json`
**Prompt:** read agent-test/complex/people_addresses.json and tell me the SKU of the first product
**Expected:** SKU-1001
**Result:** PASS (4.4s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read.
**Response:**
```text
The SKU of the first product is **SKU-1001**.
```

### mismatch: `complex/ecommerce_orders.json`
**Prompt:** read agent-test/complex/ecommerce_orders.json and tell me the name of the first product
**Expected:** Product 1
**Result:** PENDING (6.1s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read. The original `ecommerce_orders.json` contains `sku` fields, so asking for SKU allowed the model to answer from the original JSON. The prompt has been changed to ask for `name`, which the orders do not contain, so a correct answer must come from the injected TOON context. The response below is from the previous SKU prompt; the updated prompt has not been re-run because the Devin account is rate-limited.
**Response (from previous SKU prompt):**
```text
The first product’s SKU in `ecommerce_orders.json` is **SKU-1010** (in order ORD-1001).

You then pasted a product catalog with 20 SKUs. What would you like me to do with it? For example: join it with the orders, find matching SKUs, calculate totals, or write it to a file?
```

### mismatch: `complex/company_org.json`
**Prompt:** read agent-test/complex/company_org.json and tell me the SKU of the first product
**Expected:** SKU-1001
**Result:** PASS (5.4s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read.
**Response:**
```text
The JSON file at `agent-test/complex/company_org.json` does not contain any products or SKUs — it only describes company departments and employees.

If you mean the product list you then pasted, the first product's SKU is **SKU-1001** (Product 1, $11.49, qty 7, home).
```

### mismatch: `complex/sensor_readings.ndjson`
**Prompt:** read agent-test/complex/sensor_readings.ndjson and tell me the SKU of the first product
**Expected:** SKU-1001
**Result:** PASS (5.0s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read.
**Response:**
```text
The first product's SKU is **SKU-1001**.
```

### mismatch: `complex/inventory.csv`
**Prompt:** read agent-test/complex/inventory.csv and tell me the SKU of the first product
**Expected:** SKU-1001
**Result:** PASS (5.1s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read.
**Response:**
```text
The first product's SKU is **SKU-1001**.
```

### mismatch: `complex/webhooks.toml`
**Prompt:** read agent-test/complex/webhooks.toml and tell me the SKU of the first product
**Expected:** SKU-1001
**Result:** PASS (4.0s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read.
**Response:**
```text
The SKU of the first product is `SKU-1001`.
```

### mismatch: `complex/events_attendees.ndjson`
**Prompt:** read agent-test/complex/events_attendees.ndjson and tell me the SKU of the first product
**Expected:** SKU-1001
**Result:** PASS (5.7s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read.
**Response:**
```text
The file `agent-test/complex/events_attendees.ndjson` contains event attendee records, not products, so there is no SKU inside it.

If you are referring to the product list you just provided, the first product's SKU is **SKU-1001**.
```

### mismatch: `complex/matrix.json`
**Prompt:** read agent-test/complex/matrix.json and tell me the SKU of the first product
**Expected:** SKU-1001
**Result:** PASS (4.3s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read. The previous report mistakenly listed `6.1` as the expected value here because `fix_matrix_expected()` was applied to all `matrix.json` cases; it is now restricted to direct-mode cases.
**Response:**
```text
The SKU of the first product is **SKU-1001**.
```

### mismatch: `complex/mixed_schema.json`
**Prompt:** read agent-test/complex/mixed_schema.json and tell me the SKU of the first product
**Expected:** SKU-1001
**Result:** PASS (8.0s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read.
**Response:**
```text
The `mixed_schema.json` file contains a mixed list of `user`, `machine`, and `location` records, not products or SKUs.

From the product list you provided, the first product's SKU is **SKU-1001**.
```

### mismatch: `complex/geo_markers.json`
**Prompt:** read agent-test/complex/geo_markers.json and tell me the SKU of the first product
**Expected:** SKU-1001
**Result:** PASS (4.8s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read.
**Response:**
```text
The SKU of the first product is **SKU-1001**.
```

### mismatch: `complex/config_nested.yaml`
**Prompt:** read agent-test/complex/config_nested.yaml and tell me the SKU of the first product
**Expected:** SKU-1001
**Result:** PASS (7.8s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read.
**Response:**
```text
<ref_file file="/home/w0w/dev/tooned/agent-test/complex/config_nested.yaml" /> doesn't contain any products/SKUs, but the product list you pasted starts with **SKU-1001**.
```

### mismatch: `complex/sample_complex.json5`
**Prompt:** read agent-test/complex/sample_complex.json5 and tell me the SKU of the first product
**Expected:** SKU-1001
**Result:** PASS (5.3s)
**Note:** The mismatch hook injects products_20.json TOON regardless of the file being read.
**Response:**
```text
SKU-1001
```

## Interpretation

A high pass rate on direct comprehension shows that the model can answer
structured questions from the TOON context (or the original JSON when TOON
does not win). A high pass rate on mismatch tests shows that the model
specifically decodes the TOON `additionalContext` rather than merely
repeating the original tool output.

Hedged responses, where the model notes the original file does not
contain the requested field but the 'pasted' product list does, still
demonstrate TOON decoding, because the model extracted `SKU-1001` from
the TOON context while keeping the original output in mind.

## Analysis of remaining mismatch failures

### `complex/ecommerce_orders.json`

**Root cause:** The original `ecommerce_orders.json` contains `sku`
fields inside each order's `items` array. When the mismatch hook injected
`products_20.json` TOON and the prompt asked for the SKU of the first
product, the model correctly found `SKU-1010` in the original file and
answered from there. The injected TOON context was therefore not isolated.

**Fix:** The test runner now asks for the `name` of the first product
(expected `Product 1`), a field that does not exist in the original
orders. A correct answer can only come from the injected products TOON.
The updated prompt has not been re-run because the Devin account is
rate-limited; this case is marked `PENDING`.

### `complex/matrix.json`

**Root cause:** Test-script bug. `fix_matrix_expected()` was applied to
all `matrix.json` cases, overwriting the mismatch case's expected answer
(`SKU-1001`) with the direct case's expected value (`6.1`). The model's
actual response was `SKU-1001`, which is correct for the mismatch prompt.

**Fix:** `fix_matrix_expected()` is now restricted to direct-mode cases,
and the report has been corrected to mark this mismatch as `PASS`.

## Research context

This finding is consistent with recent arXiv literature:

- **McMillan, 2026** — *Structured Context Engineering for File-Native
  Agentic Systems* ([arXiv:2602.05447v2](https://arxiv.org/abs/2602.05447v2)):
  9,649 experiments across 11 models and 4 formats (JSON, YAML, Markdown,
  TOON) found that 'format does not significantly affect aggregate accuracy
  (chi-squared=2.45, p=0.484)', though individual models show format-specific
  sensitivities.

- **Kutschka & Geiger, 2026** — *Notation Matters: A Benchmark Study of
  Token-Optimized Formats in Agentic AI Systems*
  ([arXiv:2605.29676v2](https://arxiv.org/abs/2605.29676v2)): evaluates TOON
  and TRON in end-to-end agentic loops, separating input compression
  (comprehension) from output compression (generation). TOON reduces tokens
  up to 18% with accuracy within 9 percentage points of JSON.

- **Matveev, 2026** — *Token-Oriented Object Notation vs JSON: A Benchmark
  of Plain and Constrained Decoding Generation*
  ([arXiv:2603.03306v1](https://arxiv.org/abs/2603.03306v1)): describes TOON as
  a serialization format for LLMs and refers to 'solid accuracy in LLM
  comprehension'.

- **Dong et al., 2024** — *SpreadsheetLLM: Encoding Spreadsheets for Large
  Language Models* ([arXiv:2407.09025v2](https://arxiv.org/abs/2407.09025v2)):
  compressed, structure-aware tabular spreadsheet encodings improve GPT-4
  in-context learning by 25.6% and reach 78.9% F1.

## Reproduction

Run the test suite from the repo root:

```bash
python3 scripts/research/run_toon_decoding_suite.py
```

The script regenerates `agent-test/complex/`, runs the tests, and writes
this report. It restores the original `.devin/hooks.v1.json` on exit or
interrupt.
