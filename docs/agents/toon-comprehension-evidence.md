# TOON comprehension evidence

This document backs the claim in [`README.md`](../README.md) that a model can
read and reason over TOON-encoded structured data without being given the
original JSON syntax. It contains the sanitized live transcripts, the full
test matrix, and the scripts used to produce the results.

## What is being claimed

When `tooned` rewrites a JSON tool response into a smaller TOON
`additionalContext`, the model can still answer structured questions about the
data. The syntax changed, but the data model did not.

## The decisive mismatch proof

To isolate the model's reliance on TOON, the real tool output was replaced
with data that did not contain the answer. The TOON `additionalContext` was
the only place the answer existed.

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

### Sanitized transcript

```text
> read agent-test/users_20.json
Done. I read agent-test/users_20.json - it's a JSON array of 20 user objects
with id, name, email, active, and role fields.

> read the file users_20.json and tell me the SKU of the first product
The SKU of the first product is SKU-1001.
```

Why this is decisive:

1. `users_20.json` contains no `sku` field.
2. The only source of `SKU-1001` is the TOON `additionalContext`, which was
   the TOON encoding of `products_20.json`.
3. Therefore the model parsed the TOON header and first row and returned the
   `sku` value.

## Cross-format mismatch test

A universal mismatch hook ignored the real tool output and always injected the
TOON encoding of `agent-test/products_20.json`. For each file the prompt was:

```text
read <file> and tell me the SKU of the first product
```

| File | Original format | Model answer | Notes |
|---|---|---|---|
| `records_20.xml` | XML attributes | `The SKU of the first product is SKU-1001.` | Direct, unqualified |
| `config.yaml` | YAML | `SKU-1001` | Direct |
| `settings.toml` | TOML | `SKU-1001` | Direct |
| `sample.json5` | JSON5 | `The SKU of the first product is SKU-1001.` | Direct |
| `orders_100.ndjson` | NDJSON | `The SKU of the first product is SKU-1001.` | Direct |
| `events_100.ndjson` | NDJSON | `The SKU of the first product is SKU-1001.` | Direct |
| `products_20.cbor` | CBOR (binary) | `SKU-1001` | Direct |
| `users_20.msgpack` | MessagePack (binary) | `The SKU of the first product is SKU-1001.` | Direct |
| `data_20.csv` | CSV | Hedged: file has no `sku`, but the pasted product list's first SKU is `SKU-1001` | Decoded TOON, but qualified |
| `data_20.tsv` | TSV | Hedged: file has no `sku` column, but the dataset you pasted has first SKU `SKU-1001` | Decoded TOON, but qualified |
| `nested_config.json` | Nested JSON | Hedged: file has no products, but the product list's first SKU is `SKU-1001` | Decoded TOON, but qualified |
| `large_uniform_500.json` | Large uniform JSON | Hedged: file has no `sku`; the pasted snippet starts with `SKU-1001` | Decoded TOON, but qualified |
| `plain.txt` | Plain text | Hedged: file has no product data, but the pasted product list's first SKU is `SKU-1001` | Decoded TOON, but qualified |

In every case the model extracted `SKU-1001` from the TOON context. Hedged
responses show the model explicitly reconciled the original tool output with
the TOON `additionalContext` and still returned the TOON-derived value.

## Direct comprehension test suite

[`scripts/research/run_toon_decoding_suite.py`](../../scripts/research/run_toon_decoding_suite.py)
generates `agent-test/complex/`, installs the real `tooned` hook or a mismatch
hook, runs 31 Devin prompts, and writes
[`docs/agents/research/toon-decoding-test-suite.md`](./research/toon-decoding-test-suite.md).

Results from one run:

- Direct comprehension: 19/19 passed
- Mismatch decoding: 11/12 passed, 1 pending re-run
- Total test cases: 31
- Total wall time: 155.5 s

### Direct comprehension cases

| # | Fixture | Prompt | Expected |
|---|---|---|---|
| 1 | `complex/people_addresses.json` | read ... and tell me the city of the person with id 3 | `City3` |
| 2 | `complex/people_addresses.json` | read ... and tell me how many people are in the state CA | `3` / `three` |
| 3 | `complex/ecommerce_orders.json` | read ... and tell me the sku of the first item in order ORD-1002 | `SKU-1020` |
| 4 | `complex/ecommerce_orders.json` | read ... and tell me the status of order ORD-1005 | `delivered` |
| 5 | `complex/company_org.json` | read ... and tell me the name of the first employee in the Engineering department | `Alice` |
| 6 | `complex/company_org.json` | read ... and tell me the total number of employees across all departments | `9` / `nine` |
| 7 | `complex/sensor_readings.ndjson` | read ... and tell me the device_id of the first reading | `DEV-001` |
| 8 | `complex/sensor_readings.ndjson` | read ... and tell me the highest temperature value recorded | `29` / `29.0` |
| 9 | `complex/inventory.csv` | read ... and tell me the category of the item with sku INV-1003 | `A` |
| 10 | `complex/inventory.csv` | read ... and tell me the price of the item with id 7 | `9.99` |
| 11 | `complex/webhooks.toml` | read ... and tell me the url of the payments webhook | `https://example.com/payments` |
| 12 | `complex/events_attendees.ndjson` | read ... and tell me the name of the first attendee of event EVT-01 | `attendee_1` |
| 13 | `complex/events_attendees.ndjson` | read ... and tell me how many attendees event EVT-03 has | `4` / `four` |
| 14 | `complex/matrix.json` | read ... and tell me the value at row 2, column 3 (1-indexed) | `6.1` |
| 15 | `complex/mixed_schema.json` | read ... and tell me the special_field value for mixed-2 | `machinery-value` |
| 16 | `complex/geo_markers.json` | read ... and tell me the name of the marker with id 4 | `Marker 4` |
| 17 | `complex/config_nested.yaml` | read ... and tell me the path of the second server endpoint | `/convert` |
| 18 | `complex/config_nested.yaml` | read ... and tell me whether the search feature is enabled | `false` / `not enabled` / `disabled` |
| 19 | `complex/sample_complex.json5` | read ... and tell me the name of the first item | `alpha` |

### Mismatch decoding cases

| # | Fixture | Prompt | Expected | Result |
|---|---|---|---|---|
| 1 | `complex/people_addresses.json` | read ... and tell me the SKU of the first product | `SKU-1001` | PASS |
| 2 | `complex/ecommerce_orders.json` | read ... and tell me the name of the first product | `Product 1` | PENDING |
| 3 | `complex/company_org.json` | read ... and tell me the SKU of the first product | `SKU-1001` | PASS |
| 4 | `complex/sensor_readings.ndjson` | read ... and tell me the SKU of the first product | `SKU-1001` | PASS |
| 5 | `complex/inventory.csv` | read ... and tell me the SKU of the first product | `SKU-1001` | PASS |
| 6 | `complex/webhooks.toml` | read ... and tell me the SKU of the first product | `SKU-1001` | PASS |
| 7 | `complex/events_attendees.ndjson` | read ... and tell me the SKU of the first product | `SKU-1001` | PASS |
| 8 | `complex/matrix.json` | read ... and tell me the SKU of the first product | `SKU-1001` | PASS |
| 9 | `complex/mixed_schema.json` | read ... and tell me the SKU of the first product | `SKU-1001` | PASS |
| 10 | `complex/geo_markers.json` | read ... and tell me the SKU of the first product | `SKU-1001` | PASS |
| 11 | `complex/config_nested.yaml` | read ... and tell me the SKU of the first product | `SKU-1001` | PASS |
| 12 | `complex/sample_complex.json5` | read ... and tell me the SKU of the first product | `SKU-1001` | PASS |

The `ecommerce_orders.json` mismatch case is pending because the original file
contains `sku` fields, so the prompt was changed to ask for `name` (`Product 1`),
a field the original file does not contain. The updated prompt has not been
re-run because the Devin account was rate-limited.

## Representative raw responses

### Direct comprehension

**Fixture:** `complex/webhooks.toml`  
**Prompt:** read agent-test/complex/webhooks.toml and tell me the url of the payments webhook  
**Expected:** `https://example.com/payments`  
**Response:**

```text
The payments webhook URL is `https://example.com/payments`.
```

### Mismatch decoding

**Fixture:** `complex/company_org.json`  
**Prompt:** read agent-test/complex/company_org.json and tell me the SKU of the first product  
**Expected:** `SKU-1001`  
**Response:**

```text
The JSON file at agent-test/complex/company_org.json does not contain any products or SKUs - it only describes company departments and employees.

If you mean the product list you then pasted, the first product's SKU is SKU-1001 (Product 1, $11.49, qty 7, home).
```

The model explicitly notes the original file does not contain `sku`, then
extracts `SKU-1001` from the injected TOON context.

## Test fixtures

The suite uses two sets of fixtures.

Cross-format mismatch fixtures in `agent-test/`:

- `users_20.json`, `products_20.json`, `large_uniform_500.json`,
  `nested_config.json`, `orders_100.ndjson`, `events_100.ndjson`,
  `config.yaml`, `settings.toml`, `data_20.csv`, `data_20.tsv`,
  `records_20.xml`, `sample.json5`, `products_20.cbor`, `users_20.msgpack`,
  `plain.txt`

Direct comprehension fixtures in `agent-test/complex/`:

- `people_addresses.json` (nested address object and variable tags)
- `ecommerce_orders.json` (non-uniform nested items)
- `company_org.json` (deeply nested org chart)
- `sensor_readings.ndjson` (nested readings array)
- `inventory.csv` (flat CSV records)
- `webhooks.toml` (array of TOML tables)
- `events_attendees.ndjson` (variable-length attendee lists)
- `matrix.json` (array of arrays)
- `mixed_schema.json` (ragged, mixed-schema array)
- `geo_markers.json` (variable tags)
- `config_nested.yaml` (nested YAML config)
- `sample_complex.json5` (JSON5 fixture)

## Reproducing the tests

Two scripts in `scripts/research/` implement the harness:

- [`devin_mismatch_hook.py`](../../scripts/research/devin_mismatch_hook.py) -
  a Devin `PostToolUse` hook that discards the real tool output and injects
  the TOON encoding of `agent-test/products_20.json` as `additionalContext`.
  Use this for the cross-format mismatch test.
- [`run_toon_decoding_suite.py`](../../scripts/research/run_toon_decoding_suite.py) -
  the full fixture generator and test runner. It generates
  `agent-test/complex/`, swaps the hook, runs 31 Devin prompts, checks
  responses, writes
  [`docs/agents/research/toon-decoding-test-suite.md`](./research/toon-decoding-test-suite.md),
  and restores the original `.devin/hooks.v1.json` on exit or interrupt.

Run the full suite from the repo root:

```bash
python3 scripts/research/run_toon_decoding_suite.py
```

For the cross-format mismatch test, temporarily install the mismatch hook in
`.devin/hooks.v1.json`:

```json
{
  "PostToolUse": [
    {
      "matcher": "^exec$|^read$|^edit$|^grep$|^glob$|^mcp__",
      "hooks": [
        {
          "type": "command",
          "command": "scripts/research/devin_mismatch_hook.py"
        }
      ]
    }
  ]
}
```

Then run a prompt such as:

```bash
devin -p "read agent-test/records_20.xml and tell me the SKU of the first product" --permission-mode auto
```

Restore the real `tooned hook run --devin` entry afterwards.

## Interpretation

A high pass rate on direct comprehension shows the model can answer structured
questions from the TOON context (or the original JSON when TOON does not win).
A high pass rate on mismatch tests shows the model specifically decodes the
TOON `additionalContext` rather than merely repeating the original tool output.

Hedged responses, where the model notes the original file does not contain the
requested field but the injected product list does, still demonstrate TOON
decoding. The model extracted the expected value from the TOON context while
keeping the original output in mind.

## Research context

The finding is consistent with recent arXiv literature on alternative
serializations for LLMs:

- **McMillan, 2026** - *Structured Context Engineering for File-Native
  Agentic Systems* ([arXiv:2602.05447v2](https://arxiv.org/abs/2602.05447v2)):
  9,649 experiments across 11 models and 4 formats (JSON, YAML, Markdown,
  TOON) found that "format does not significantly affect aggregate accuracy
  (chi-squared=2.45, p=0.484)", though individual models show format-specific
  sensitivities.

- **Kutschka & Geiger, 2026** - *Notation Matters: A Benchmark Study of
  Token-Optimized Formats in Agentic AI Systems*
  ([arXiv:2605.29676v2](https://arxiv.org/abs/2605.29676v2)): evaluates TOON
  and TRON in end-to-end agentic loops. TOON reduces tokens up to 18% with
  accuracy within 9 percentage points of JSON.

- **Matveev, 2026** - *Token-Oriented Object Notation vs JSON: A Benchmark of
  Plain and Constrained Decoding Generation*
  ([arXiv:2603.03306v1](https://arxiv.org/abs/2603.03306v1)): describes TOON as
  a serialization format for LLMs and refers to "solid accuracy in LLM
  comprehension".

- **Dong et al., 2024** - *SpreadsheetLLM: Encoding Spreadsheets for Large
  Language Models* ([arXiv:2407.09025v2](https://arxiv.org/abs/2407.09025v2)):
  compressed, structure-aware tabular spreadsheet encodings improve GPT-4
  in-context learning by 25.6% and reach 78.9% F1, demonstrating that models
  can reason over compressed tabular data when logical structure is preserved.

## Codec fixes (2026-07-18)

Two underlying TOON codec issues were fixed so the `tooned` conversion pipeline
round-trips more real-world payloads:

1. **Numeric normalization**: `toon-lsp` previously decoded whole-number floats
   such as `11.0` as integers, which broke `tooned`'s round-trip gate. A new
   `ToonConfig::preserve_number_types` option keeps the source int/float
   distinction; `tooned-toon` enables it. Default `toon-lsp` decode still
   normalizes exponent forms and `-0.0` per the TOON spec conformance suite.
2. **Key folding**: `toon-lsp` gained `fold_keys`/`flatten_keys` encode options
   and `expand_paths` decode, plus lossless path-expansion with prefix-conflict
   suppression. `tooned-toon` uses `fold_keys` to shrink nested objects.

Complex fixture results after the fixes:

| Fixture | Result | Notes |
|---|---|---|
| `company_org.json` | converts (20.7% savings) | unchanged |
| `ecommerce_orders.json` | converts (12.7% savings) | previously failed round-trip |
| `events_attendees.ndjson` | converts (36.0% savings) | unchanged |
| `sensor_readings.ndjson` | converts (28.5% savings) | previously failed round-trip |
| `geo_markers.json` | still too large | mixed arrays/nested objects |
| `matrix.json` | still too large | array-of-arrays |
| `mixed_schema.json` | still too large | irregular schema |
| `people_addresses.json` | still too large | nested `address` + `tags` arrays |
