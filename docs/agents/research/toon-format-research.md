# TOON Format Research — Nested and Complex Structures

This document explains why some `agent-test` fixtures do not convert to TOON and whether that indicates a bug in `tooned`.

## Conclusion

No `tooned` code change is needed. The fixtures that do not convert are the ones the TOON specification itself says JSON is usually better for: deeply nested objects, non-uniform arrays, arrays of arrays, and payloads where TOON does not beat compact JSON by the configured margin. `tooned` correctly rejects them because the conversion gate requires the TOON encoding to be smaller *and* `decode(encode(x)) == x`.

## The conversion hot path

`crates/tooned-convert/src/lib.rs` runs the same pipeline for every tool output:

1. `detect()` identifies the input format.
2. `parse_by_doc_type()` parses it into a `serde_json::Value`.
3. `shape::classify()` samples the value for reporting only.
4. `encode_toon_raw_with_options()` in `crates/tooned-toon/src/lib.rs` calls `toon_lsp::toon::encode_with_config`.
5. `apply_dict()` optionally compresses repeated cell values into a `legend:` block when `dict_enabled` is true.
6. `maybe_tooned()` compares the compact-JSON byte count to the TOON byte count and accepts TOON only when it is smaller by the configured margin and the round-trip decode reproduces the original value exactly.

If any step fails, `maybe_tooned` returns `Conversion::Passthrough` and the model sees the original bytes.

## Upstream TOON implementation

- `tooned` depends on `toon-lsp = "0.7.21"`, which depends on `toon-format = "0.5.0"`.
- `toon_lsp::toon::ToonConfig` controls `fold_keys`, `flatten_keys`, `expand_paths`, and `preserve_number_types`. `tooned-toon` maps `ConversionOptions` to `ToonConfig` and defaults `fold_keys=true`, `expand_paths=true`, and `preserve_number_types=true` so nested single-key objects and whole-number floats round-trip.
- The library `ConversionOptions` defaults also have `dict_enabled=true`, but `auto_margin=false` and `entropy_gate=false`. The `tooned` CLI and hook override those to `auto_margin=true` and `entropy_gate=true`, so `tooned check` reports the same defaults an end user sees.
- The TOON spec supports nested objects, expanded arrays, and arrays of arrays, but tabular arrays (`key[N]{f1,f2}:`) require identical field sets across all objects and primitive values in the declared fields.

Sources:

- [TOON Specification](https://github.com/toon-format/spec/blob/main/SPEC.md) (the `toon-format` 0.5.0 crate implements TOON v3.0; the spec repo is at v3.3.0)
- [toon-format/toon-rust](https://github.com/toon-format/toon-rust)
- [toonformat.dev format overview](https://toonformat.dev/guide/format-overview.html)
- [crates.io: toon-lsp](https://crates.io/crates/toon-lsp)

## Complex fixture conversion results

The `tooned check` results below are from the current build:

| Fixture | `tooned check` result | Why it did / did not convert |
|---|---|---|
| `complex/people_addresses.json` | no — TOON 1595 B vs JSON 1368 B | Each person object contains a nested `address` object and a `tags` array, so the encoder cannot emit a smaller tabular form. |
| `complex/ecommerce_orders.json` | yes (12.7%) | Nested `items` arrays now convert with the current encoder. |
| `complex/company_org.json` | yes (20.7%) | Deeply nested org chart folds cleanly. |
| `complex/matrix.json` | no — TOON 160 B vs JSON 121 B | Top-level array of arrays of numbers; JSON is smaller for this shape. |
| `complex/sensor_readings.ndjson` | yes (28.5%) | Nested `readings` arrays per row now convert with the current encoder. |
| `complex/mixed_schema.json` | no — TOON 247 B vs JSON 231 B | Irregular, mixed-schema array; TOON cannot beat compact JSON. |
| `complex/geo_markers.json` | no — TOON 871 B vs JSON 760 B | Variable tags make the objects non-uniform. |
| `complex/webhooks.toml` | no — TOON 285 B vs JSON 283 B | Array of TOML tables; the difference is within the margin. |
| `complex/sample_complex.json5` | no — TOON 122 B vs JSON 119 B | JSON5 is detected but the TOON encoding is slightly larger. |

The `complex/inventory.csv` (55.4%), `complex/config_nested.yaml` (11.0%), and `complex/events_attendees.ndjson` (36.2%) fixtures also convert. Those results are in [`toon-evidence.md`](../toon-evidence.md).

## The `ecommerce_orders.json` mismatch ambiguity

In the mismatch test, `complex/ecommerce_orders.json` was the only ambiguous result. Its `items` arrays contain `sku` fields, so when the prompt asked for "the SKU of the first product" the model correctly answered from the original JSON (`SKU-1010`) instead of the injected TOON (`SKU-1001`). The fix is to ask for a field the original file lacks, such as the product `name` (`Product 1`). This is a prompt-design issue, not a `tooned` bug.

## What could improve conversion for nested data

Most remaining non-conversions are expected, but if nested TOON compression becomes a priority, the relevant levers are:

1. **Key folding** is already enabled by default. Deeper non-uniform nesting is the remaining gap.
2. **Arrays of arrays** (`matrix.json`) are genuinely smaller in JSON; a special matrix encoding or leaving them as JSON is the right behavior.
3. **Dictionary tier** (`dict_enabled`) can inline repeated cell values. It already helps fixtures like `ecommerce_orders.json` and `sensor_readings.ndjson` convert; for shapes it cannot yet round-trip, a more conservative fallback could let more payloads through.
4. **Do nothing for deeply nested data.** The TOON spec explicitly recommends JSON for deeply nested or non-uniform structures, so `tooned`'s passthrough behavior is spec-aligned.

## Research context

- **McMillan, 2026** — *Structured Context Engineering for File-Native Agentic Systems* (arXiv:2602.05447v2): 9,649 experiments across 11 models and 4 formats (JSON, YAML, Markdown, TOON) found "format does not significantly affect aggregate accuracy (chi-squared=2.45, p=0.484)," though individual models show format-specific sensitivities.
- **Kutschka & Geiger, 2026** — *Notation Matters: A Benchmark Study of Token-Optimized Formats in Agentic AI Systems* (arXiv:2605.29676v2): TOON reduces tokens up to 18% with accuracy within 9 percentage points of JSON in end-to-end agentic loops.
- **Matveev, 2026** — *Token-Oriented Object Notation vs JSON* (arXiv:2603.03306v1): describes TOON as a serialization for LLMs and notes "solid accuracy in LLM comprehension."
- **Dong et al., 2024** — *SpreadsheetLLM* (arXiv:2407.09025v2): a compressed, structure-aware tabular encoding improves GPT-4 in-context learning by 25.6% and reaches 78.9% F1.
