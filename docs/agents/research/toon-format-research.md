# TOON Format Research — Nested and Complex Structures

This document records the research performed with `codegraph`, `context7`, `exa`,
and `websearch` into the root causes of the failures observed in the live
`toon-decoding-test-suite`. The goal was to determine whether any failure
required a change to `tooned` itself.

## Conclusion

No `tooned` code change was required for any of the observed failures. The
direct-comprehension failures were test-expectation errors, and the two
mismatch failures were a test-prompt design issue and a test-script bug. The
`tooned` converter behaved correctly according to the TOON specification and
the upstream `toon-lsp`/`toon-format` implementation it depends on.

## Methodology

- `codegraph` on `/home/w0w/dev/tooned` to trace the conversion hot path
  (`maybe_tooned` → `attempt` → `encode_toon` → `toon_lsp::toon::encode`).
- `context7`/`exa`/`websearch` for current TOON spec and upstream crate
  documentation.
- `thoughtbox` to synthesize the failure root-cause analysis.

## The conversion hot path

`tooned` converts tool output in `crates/tooned-convert/src/lib.rs`:

1. `detect()` identifies the input format (JSON, YAML, CSV, NDJSON, XML, TOML,
   JSON5, MessagePack, CBOR).
2. `parse_by_doc_type()` parses the bytes into a `serde_json::Value`.
3. `shape::classify()` samples the value for reporting only; it does **not**
   gate conversion.
4. `encode_toon()` (in `crates/tooned-toon/src/lib.rs`) calls
   `toon_lsp::toon::encode(value)`.
5. `maybe_tooned()` compares the original JSON byte count to the TOON byte
   count and picks TOON only when it is smaller **and** round-trips back to the
   exact same `Value`.

If `toon_lsp::toon::encode` cannot produce a smaller, round-trippable TOON
string, `maybe_tooned` returns `Conversion::Passthrough` and the model sees the
original JSON. This is the intended fail-safe behavior.

## Upstream TOON implementation

- `tooned` depends on `toon-lsp = "0.6"` (crates.io). `toon-lsp` is described
  as "a Language Server Protocol implementation for TOON" and exposes
  `toon_lsp::toon::encode`/`decode`.
- The published `toon-lsp` 0.6 crate depends on `toon-format = "^0.4"`
  (per crates.io metadata), which is the official spec-compliant Rust
  implementation of TOON v3.x.
- The TOON specification (v3.3) supports nested objects, expanded arrays, and
  arrays of arrays, but **tabular arrays** (`key[N]{f1,f2}:`) require:
  - identical field sets across all objects,
  - **primitive values only** in the declared fields (no nested objects or
    arrays).

Sources:

- [TOON Specification v3.3](https://github.com/toon-format/spec/blob/main/SPEC.md)
- [toon-format/toon-rust](https://github.com/toon-format/toon-rust)
- [toonformat.dev format overview](https://toonformat.dev/guide/format-overview.html)
- [crates.io: toon-lsp](https://crates.io/crates/toon-lsp)

## Why the complex fixtures did not convert

We confirmed this with `tooned convert --to toon <file>` on each fixture:

| Fixture | Why it did not convert |
|---|---|
| `complex/people_addresses.json` | Each person object contains a nested `address` object and a `tags` array. TOON tabular encoding requires primitive fields only, so the encoder cannot emit a smaller tabular form. It falls back to passthrough. |
| `complex/ecommerce_orders.json` | Each order contains a nested `items` array of objects. The structure is non-uniform and non-primitive, so TOON cannot beat compact JSON. |
| `complex/company_org.json` | Deeply nested object with departments and employees. This is exactly the "deeply nested or non-uniform structures" case the TOON spec says JSON is often better for. |
| `complex/matrix.json` | Top-level array of arrays of numbers. `toon-lsp` does not produce a smaller encoding than JSON for this shape, so it passthroughs. |
| `complex/sensor_readings.ndjson` | Nested `readings` array per row. Passthrough. |

For CSV, TSV, and flat NDJSON fixtures, `tooned` **did** convert to TOON and
injected it as `additionalContext`. Those direct-comprehension prompts passed
because the model decoded the TOON header/row format.

## The mismatch test design issue

The one genuine mismatch failure was `complex/ecommerce_orders.json`. The
mismatch hook always injects `products_20.json` TOON, but the original
`ecommerce_orders.json` also contains `sku` fields. When the prompt asked for
"the SKU of the first product", the model correctly answered from the original
JSON (`SKU-1010`) instead of from the injected TOON (`SKU-1001`). The fix is to
ask for a field the original file does not contain (`name` → `Product 1`), as
done in PR #47.

The `matrix.json` mismatch failure was a test-script bug:
`fix_matrix_expected()` was overwriting the mismatch case's expected answer
with the direct case's expected value. The model's actual response was
`SKU-1001`, which is correct.

## Research-backed context

The literature already predicts that models can read alternative structured
serializations without the original JSON syntax:

- **McMillan, 2026** — *Structured Context Engineering for File-Native
  Agentic Systems* ([arXiv:2602.05447v2](https://arxiv.org/abs/2602.05447v2)):
  9,649 experiments across 11 models and 4 formats (JSON, YAML, Markdown,
  TOON) found "format does not significantly affect aggregate accuracy
  (chi-squared=2.45, p=0.484)".
- **Kutschka & Geiger, 2026** — *Notation Matters: A Benchmark Study of
  Token-Optimized Formats in Agentic AI Systems*
  ([arXiv:2605.29676v2](https://arxiv.org/abs/2605.29676v2)): TOON reduces
  tokens up to 18% with accuracy within 9 percentage points of JSON in
  end-to-end agentic loops.
- **Matveev, 2026** — *Token-Oriented Object Notation vs JSON*
  ([arXiv:2603.03306v1](https://arxiv.org/abs/2603.03306v1)): describes TOON
  as a serialization format for LLMs and notes "solid accuracy in LLM
  comprehension".
- **Dong et al., 2024** — *SpreadsheetLLM*
  ([arXiv:2407.09025v2](https://arxiv.org/abs/2407.09025v2)): compressed,
  structure-aware tabular encodings improve GPT-4 in-context learning by
  25.6% and reach 78.9% F1, showing models can reason over compressed
  tabular data when logical structure is preserved.

## What would improve `tooned` for nested data?

The test failures did **not** require these changes, but the research surfaced
future directions if nested TOON compression becomes a priority:

1. **Flatten nested objects into dotted keys.** The `toon-format` crate
   supports `KeyFoldingMode::Safe`, which collapses single-key object chains
   (`{"a":{"b":1}}` → `a.b: 1`). `tooned` could optionally pre-flatten nested
   objects before encoding, then expand dotted keys on decode. This helps
   configs like `company_org.json` but must be gated by a round-trip check.

2. **Use `toon-format` directly with explicit options.** `tooned` currently
   calls `toon_lsp::toon::encode` with default options. Calling
   `toon_format::encode` with `with_key_folding` and `with_flatten_depth`
   could shrink nested payloads, but it would require a license/audit update
   (`toon-format` is MIT) and careful `deny.toml`/`supply-chain` updates.

3. **Pre-process nested arrays into ONTO/TRON.** `tooned` already has
   `maybe_onto` and `maybe_tron` paths for columnar/streaming data. For
   large nested NDJSON or event streams, routing through TRON may be more
   token-efficient than the generic `maybe_tooned` path.

4. **Do nothing for deeply nested data.** The TOON spec itself recommends
   JSON for deeply nested or non-uniform structures. `tooned`'s current
   passthrough behavior is spec-aligned and safe.

## Final verdict

The failures in `toon-decoding-test-suite.md` were explained by the test suite
itself, not by `tooned`. The converter is doing the right thing: it only emits
TOON when the representation is smaller and losslessly round-trips. Complex
nested fixtures naturally fall back to JSON, which is consistent with the TOON
spec and the upstream encoder's design.
