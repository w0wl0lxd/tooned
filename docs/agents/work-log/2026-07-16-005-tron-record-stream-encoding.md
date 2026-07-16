# 005 TRON record-stream encoding

- **Date:** 2026-07-16
- **Author:** w0wl0lxd
- **Branch:** `004-tron-encoding`
- **PR(s):** #13

## Context

PR2 from the four-PR backlog: implement the TRON (Token-Reduced Object Notation) record-stream encoding that was previously only a placeholder in `tooned convert --to tron`. TRON is the complement to ONTO in the ONTO/TRON encoding family: where ONTO is a pipe-delimited columnar format for uniform arrays, TRON hoists repeated object schemas into a `class A: field1, field2` header and emits each record as a compact `A(value, value, ...)` instantiation. The body remains a JSON superset so existing tooling can still parse values that do not benefit from class compression.

## Reasoning

- A class-header design matches the public TRON format descriptions used by the `tron-format` and `tron-python` reference implementations, while keeping the first implementation small and self-contained in Rust.
- Restricting the prototype encoder to flat objects (and flat uniform arrays) preserves round-trip fidelity and size-based gating through the existing `maybe_*` conversion pipeline.
- The decoder is general enough to expand any `ClassName(...)` calls found in a JSON-with-calls body, including nested calls and calls inside JSON objects/arrays, so the format can grow later without a decoder rewrite.
- `tooned convert --to json` now recognizes TRON by its `class ` header and decodes it to compact JSON, completing the two-way path.

## Steps taken

1. Added `crates/tooned-convert/src/tron.rs` with:
   - `encode(value)`: emits `class A: <keys>` followed by a blank line and `A(...)` instances for single objects or arrays.
   - `decode(text)`: parses the class header, then expands class calls into JSON objects and parses the body as JSON.
   - `maybe_tron(input, opts)`: conversion pipeline that respects `max_input_bytes`, `margin_pct`, and round-trip checks, mirroring `maybe_onto`.
2. Re-exported TRON functions from `crates/tooned-convert/src/lib.rs` and `crates/tooned-core/src/lib.rs`.
3. Wired `--to tron` in `crates/tooned-cli/src/cli/convert.rs` to call `maybe_tron` and write the result or passthrough bytes, and updated `--to json` decoding to detect TRON by its `class ` header.
4. Added unit tests for `encode`/`decode`/`maybe_tron` and CLI integration tests for `convert --to tron` and `convert --to json` from a TRON file.

## Verification

```bash
cd /home/w0w/dev/tooned
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo vet
```

Observed output:

- `cargo fmt --all` — PASS (exit 0)
- `cargo clippy --all-targets --all-features -- -D warnings` — PASS (exit 0)
- `cargo nextest run --all-features` — `254 tests run: 254 passed, 1 skipped`
- `cargo vet` — `Vetting Succeeded (38 fully audited, 1 partially audited, 251 exempted)`

Manual TRON smoke test:

```bash
echo '[{"id":0,"name":"row-0","active":true,"score":0.5},
       {"id":1,"name":"row-1","active":false,"score":1.5}]' | \
  cargo run --quiet -- convert - --to tron
```

Produces the expected `class A:` header and compact `A(...)` records.

## PR description

Implements the TRON record-stream encoding for flat objects and uniform arrays behind `tooned convert --to tron` and adds `tooned convert --to json` decoding for TRON input. All verification gates pass.

## Follow-ups

- PR3: heapster zero-allocation hot-path assertions.
- PR4: streaming NDJSON/JSONL conversion for large inputs.
- Extend the TRON encoder to support multiple class definitions and nested object shapes when needed.

## Changelog

Inserted under `### Added` in [CHANGELOG.md](../../../CHANGELOG.md).
