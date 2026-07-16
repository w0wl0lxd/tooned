# 2026-07-16-006: Streaming NDJSON/JSONL Conversion

## Context
The `tooned convert` subcommand previously used bounded conversion for all inputs, loading the entire file into memory. For large NDJSON/JSONL files (e.g., >2 MiB), this could cause memory pressure. The goal was to add streaming support for NDJSON/JSONL inputs to handle large files efficiently.

## Implementation

### Core Changes

**crates/tooned-convert/src/tron.rs**
- Added `maybe_tron_stream<R, W>(reader: R, writer: W) -> Result<StreamStats, ToonedError>` function that streams NDJSON to TRON
- Uses `tooned_json::parse_ndjson_stream` to parse input line-by-line
- Writes TRON output incrementally with a class header from the first object
- Returns `StreamStats { input_bytes, output_bytes }` for size comparison
- On parse error, writes plain JSON fallback for the problematic value
- Fixed clippy warnings: redundant closure, while-let-on-iterator, collapsible-if, if-not-else

**crates/tooned-convert/src/lib.rs**
- Made `is_smaller_enough` public so it can be used from `tooned-cli` for adaptive streaming

**crates/tooned-core/src/lib.rs**
- Re-exported `maybe_tron_stream`, `StreamStats`, `parse_ndjson_stream`, and `is_smaller_enough` for CLI use

**crates/tooned-cli/Cargo.toml**
- No new runtime dependencies added; streaming reuses existing `std::io` temp-file helpers

**crates/tooned-cli/src/cli/mod.rs**
- Added `#[derive(PartialEq)]` to `FormatHint` enum to enable direct comparison in `convert.rs`

**crates/tooned-cli/src/cli/convert.rs**
- Added helper functions:
  - `get_input_size(input: &Path) -> u64`: Returns file size or 0 for stdin
  - `is_ndjson_extension(path: &Path) -> bool`: Checks for `.ndjson` or `.jsonl` extensions
  - `TempFile`: RAII guard that deletes a temp file on drop unless its path is taken for promotion
  - `unique_temp_path(dir: &Path, prefix: &str) -> PathBuf`: Builds a unique temp path from pid + nanoseconds
  - `spool_stdin_to_temp() -> anyhow::Result<(TempFile, std::fs::File, u64)>`: Spools stdin to a temp file and returns an open reader + size
  - `open_streaming_output(out: Option<&Path>) -> anyhow::Result<(TempFile, Box<dyn std::io::Write>)>`: Opens a buffered temp writer in the output directory (or system temp for stdout)
  - `copy_input_to_output(input: &Path, out: Option<&Path>) -> anyhow::Result<()>`: Copies the original input to the destination without buffering it in memory, skipping when input == output
  - `run_tron_streaming(args: &ConvertArgs, opts: &ConversionOptions) -> anyhow::Result<()>`: Handles `--to tron` forced streaming
  - `run_adaptive_streaming(args: &ConvertArgs, opts: &ConversionOptions) -> anyhow::Result<()>`: Handles default adaptive streaming
- Modified `run_convert` to:
  - Use streaming when `--to tron` is forced with NDJSON hint/extension
  - Use streaming when input is large (above `max_input_bytes`) and format is NDJSON
  - Fall back to bounded path for small NDJSON inputs
- Streaming implementation details:
  - For stdin: spools stdin to a temp file first to allow retry/copy on error
  - Writes streamed output to a buffered temp file
  - For `--to tron`: promotes temp file atomically for file output, copies to stdout for stdout output
  - For adaptive: compares output size vs input size using `is_smaller_enough`, discards temp and passthrough if not smaller enough
  - On parse/IO error: falls back to passthrough of original input
  - No new `tempfile` crate dependency; temp files are managed with `std::fs` and a private `TempFile` guard

### Test Coverage

**crates/tooned-cli/tests/cli_convert.rs**
- Added 6 new integration tests:
  - `convert_to_tron_on_ndjson_with_format_hint_produces_tron_stream`: Tests `--format-hint ndjson` with `--to tron`
  - `convert_to_tron_on_ndjson_extension_produces_tron_stream`: Tests `.ndjson` extension with `--to tron`
  - `convert_to_tron_on_jsonl_extension_produces_tron_stream`: Tests `.jsonl` extension with `--to tron`
  - `convert_to_tron_round_trips_ndjson_via_json`: Tests NDJSON → TRON → JSON round-trip fidelity
  - `large_ndjson_file_converts_with_to_tron`: Tests streaming with 10,000-line NDJSON file (>2 MiB)
  - `adaptive_streaming_chooses_tron_when_smaller`: Tests adaptive path chooses TRON for uniform data
  - `adaptive_streaming_passthrough_when_not_smaller_enough`: Tests adaptive path passthrough for small inputs
- Fixed clippy warnings in tests: format-push-string, cast-lossless, write-with-newline

## Verification

All tests pass:
```bash
$ cargo nextest run --all-features --no-fail-fast
────────────
     Summary [   3.076s] 262 tests run: 262 passed, 1 skipped
```

Clippy passes with `-D warnings`:
```bash
$ cargo clippy --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.31s
```

## Notes

- Streaming is only used when the input is large (above `max_input_bytes`) or when `--to tron` is explicitly forced with an NDJSON hint/extension
- Small NDJSON inputs continue to use the bounded path for simplicity
- The implementation uses temp files for both input (stdin) and output, ensuring atomic operations and error recovery
- On parse/IO errors, the original input is passed through unchanged, ensuring robustness
