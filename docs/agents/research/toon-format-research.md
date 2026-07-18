# TOON Format Research — Nested and Complex Structures

This document records the research performed with `context7`, `exa`, and direct `tooned` experiments into why some fixtures do or do not convert to TOON. The goal is to determine whether any failure requires a change to `tooned` itself.

## Conclusion

No `tooned` code change is required for the remaining conversion failures. The fixtures that do not convert are the ones the TOON specification itself says JSON is usually better for: deeply nested objects, non-uniform arrays, arrays of arrays, and structures where the TOON byte count does not beat compact JSON by the effective margin. The `tooned` converter behaves correctly: it only emits TOON when the representation is smaller and losslessly round-trips.

## Methodology

- `context7` and `exa` for the current TOON v3.3 specification and upstream implementation notes.
- `tooned check <fixture>` on each `agent-test` fixture to get the actual conversion decision and `PassthroughReason`.
  The `agent-test/` fixtures are generated locally and excluded from version control, so they must exist before running `tooned check`.
- Source reading of `crates/tooned-convert/src/lib.rs` and `crates/tooned-toon/src/lib.rs` to trace the conversion hot path.

## The conversion hot path

`tooned` converts tool output in `crates/tooned-convert/src/lib.rs`:

1. `detect()` identifies the input format (JSON, NDJSON, YAML, TOML, CSV, TSV, XML, JSON5, CBOR, MessagePack, plain text).
2. `parse_by_doc_type()` parses the bytes into a `serde_json::Value`.
3. `shape::classify()` samples the value for reporting only; it does **not** gate conversion.
4. `encode_toon_raw_with_options(value, opts)` in `crates/tooned-toon/src/lib.rs` builds a `ToonConfig` from `opts` and calls `toon_lsp::toon::encode_with_config(value, &toon_config(opts))`.
5. `maybe_tooned()` compares the original compact-JSON byte count to the TOON byte count and picks TOON only when it is smaller by the effective margin *and* decoding that TOON back reproduces the original value exactly.

If `toon_lsp::toon::encode` cannot produce a smaller, round-trippable TOON string, `maybe_tooned` returns `Conversion::Passthrough` and the model sees the original bytes untouched.

## Upstream TOON implementation

- `tooned` depends on `toon-lsp = "0.7.21"` (crates.io). `toon-lsp` is described as "a Language Server Protocol implementation for TOON" and exposes `toon_lsp::toon::encode_with_config` / `decode_with_config`.
- `toon-lsp` 0.7.21 depends on `toon-format = "0.5"`, which is the spec-compliant Rust implementation of TOON v3.x.
- `toon_lsp::toon::ToonConfig` controls `fold_keys`, `flatten_keys`, `expand_paths`, and `preserve_number_types`. `tooned-toon` maps `ConversionOptions` to `ToonConfig` and defaults `fold_keys=true`, `expand_paths=true`, and `preserve_number_types=true` so nested single-key objects and whole-number floats round-trip losslessly.
- The TOON specification (v3.3) supports nested objects, expanded arrays, and arrays of arrays, but **tabular arrays** (`key[N]{f1,f2}:`) require identical field sets across all objects and primitive values in the declared fields.

Sources:

- [TOON Specification v3.3](https://github.com/toon-format/spec/blob/main/SPEC.md)
- [toon-format/toon-rust](https://github.com/toon-format/toon-rust)
- [toonformat.dev format overview](https://toonformat.dev/guide/format-overview.html)
- [crates.io: toon-lsp](https://crates.io/crates/toon-lsp)

## Why the complex fixtures behave as they do

The `tooned check` results below are from the current build:

| Fixture | `tooned check` result | Why it behaves this way |
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

For CSV, TSV, and flat NDJSON fixtures such as `events_100.ndjson` and `events_attendees.ndjson`, `tooned` **did** convert to TOON. With an agent protocol that replaces the tool result with TOON, the model could read the tabular header/row format. `tooned` does not use `additionalContext` in `PostToolUse` because that would keep the original JSON in context alongside the TOON, inflating total token count.

## The mismatch test design issue

The one ambiguous mismatch result was `complex/ecommerce_orders.json`. The mismatch hook always replaces the tool result with `products_20.json` TOON, but the original `ecommerce_orders.json` also contains `sku` fields. When the prompt asked for "the SKU of the first product", the model correctly answered from the original JSON (`SKU-1010`) instead of from the replacement TOON (`SKU-1001`). The fix is to ask for a field the original file does not contain, such as `name` (`Product 1`).

The `matrix.json` mismatch result was previously marked as a failure because the test script expected `6.1` (the direct-comprehension expected value) rather than `SKU-1001`. The model's actual response was `SKU-1001`, which is correct for the mismatch prompt.

## Research-backed context

The literature already predicts that models can read alternative structured serializations without the original JSON syntax:

- **McMillan, 2026** — *Structured Context Engineering for File-Native Agentic Systems* (arXiv:2602.05447v2): 9,649 experiments across 11 models and 4 formats (JSON, YAML, Markdown, TOON) found "format does not significantly affect aggregate accuracy (chi-squared=2.45, p=0.484)."
- **Kutschka & Geiger, 2026** — *Notation Matters: A Benchmark Study of Token-Optimized Formats in Agentic AI Systems* (arXiv:2605.29676v2): TOON reduces tokens up to 18% with accuracy within 9 percentage points of JSON in end-to-end agentic loops.
- **Matveev, 2026** — *Token-Oriented Object Notation vs JSON* (arXiv:2603.03306v1): describes TOON as a serialization format for LLMs and notes "solid accuracy in LLM comprehension."
- **Dong et al., 2024** — *SpreadsheetLLM* (arXiv:2407.09025v2): a compressed, structure-aware tabular encoding improves GPT-4 in-context learning by 25.6% and reaches 78.9% F1.

## What would improve `tooned` for nested data?

The current conversion failures do **not** require these changes, but the research surfaced future directions if nested TOON compression becomes a priority:

1. **Deeper key folding is already enabled.** `tooned-toon` already maps `ConversionOptions` to `ToonConfig` with `fold_keys=true` and `expand_paths=true`, so single-key object chains (`{"user":{"name":"x"}}` → `user.name: x`) round-trip. Deeper non-uniform nesting is the remaining gap.
2. **Arrays of arrays.** `matrix.json` is an example where JSON is genuinely smaller. TOON's tabular format is not designed for this shape; a special matrix encoding or simply leaving it as JSON is the right behavior.
3. **Dictionary tier refinements.** `tooned-toon` applies `apply_dict` when `dict_enabled` is true, while protecting keys that match the critical-field policy (e.g., `order_id` matching the `id` substring). With `preserve_number_types` and key folding enabled, many nested shapes now round-trip successfully; a more conservative dict fallback could still help for the remaining non-uniform cases.
4. **Do nothing for deeply nested data.** The TOON spec itself recommends JSON for deeply nested or non-uniform structures. `tooned`'s current passthrough behavior is spec-aligned and safe.

## Final verdict

The remaining conversion failures are explained by the TOON specification and the actual `tooned` conversion gate, not by a bug in `tooned`. The converter is doing the right thing: it only emits TOON when the representation is smaller and losslessly round-trips. Complex nested fixtures that still do not convert naturally fall back to JSON, which is consistent with the TOON spec and the upstream encoder's design.
