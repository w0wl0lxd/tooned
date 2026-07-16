# 005 heapster zero-allocation hot-path assertions

- **Date:** 2026-07-16
- **Author:** w0wl0lxd
- **Branch:** `005-heapster-zero-alloc`
- **PR(s):** #14

## Context

PR3 from the four-PR backlog: add heap telemetry to the conversion hot path so that future regressions that introduce heap allocations in `tooned-detect` are caught by tests, not by profiling later.

## Reasoning

- `tooned-detect::detect` is the first gate in every conversion path and is intended to be a zero-allocation, byte-slice-only decision. If it ever starts allocating, the latency and memory profile of the whole pipeline degrades.
- `heapster` is a lightweight, atomic counter wrapper over a `GlobalAlloc` that is specifically designed for this use case: measuring allocation counts and byte sums inside a closure without heavy profiling overhead.
- Wrapping the test binary's allocator with `heapster` and asserting `diff.alloc_count == 0` / `diff.alloc_sum == 0` for representative inputs gives a durable, automated guardrail.

## Steps taken

1. Added `heapster = "0.8.0"` as a dev-dependency of `crates/tooned-detect`.
2. Installed a test-only global allocator in `crates/tooned-detect/src/lib.rs` under `#[cfg(test)]`.
3. Added `detect_is_zero_allocation_on_representative_inputs` covering JSON, NDJSON, YAML, TOML, CSV, TSV, unstructured text, and empty input.
4. Added a `cargo vet` exemption for `heapster:0.8.0` with `safe-to-run` criteria (dev-only).

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
- `cargo nextest run --all-features` — `247 tests run: 247 passed, 1 skipped`
- `cargo vet` — `Vetting Succeeded (38 fully audited, 1 partially audited, 252 exempted)`

The new test specifically asserts `diff.alloc_count == 0` and `diff.alloc_sum == 0` for every representative input.

## PR description

Adds `heapster` as a `tooned-detect` dev-dependency and asserts that the `detect` sniffing hot path performs zero heap allocations across JSON, NDJSON, YAML, TOML, CSV, TSV, unstructured, and empty inputs. Includes the corresponding `cargo vet` exemption.

## Follow-ups

- PR4: streaming NDJSON/JSONL conversion for large inputs.
- Consider `heapster` assertions for other hot paths once their allocation budget is defined.

## Changelog

Inserted under `### Added` in [CHANGELOG.md](../../../CHANGELOG.md).
