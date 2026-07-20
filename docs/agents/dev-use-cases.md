# Developer use cases and suggestions

Based on actual testing (`tooned check`, `tooned pipe`, `tooned index`, streaming scenarios, edge cases) and codebase exploration (`crates/tooned-json/src/lib.rs`, `crates/tooned-convert/src/lib.rs`, `crates/tooned-metrics/src/store.rs`, `crates/tooned-cli/src/config.rs`).

## Real-time streaming applications

**Use case:** Web analytics event streams, IoT telemetry, clickstream data.

**Approach:** Use `parse_ndjson_stream` (not `parse_ndjson`) for memory-efficient processing. The iterator yields one event per line without buffering the full stream.

**Verified:** `cat agent-test/events_100.ndjson | tooned pipe` produces 3057 bytes (52.4% structured savings, 60.9% raw file savings). The `bytes_read()` counter tracks all consumed bytes including newlines.

**Suggestion:** For production streaming, combine `tooned wrap -- <producer>` with a consumer that reads the converted stream line-by-line. Monitor `bytes_read()` for throughput metrics.

## Large dataset batch processing

**Use case:** Database exports, inventory lists, sensor reading dumps.

**Approach:** Use `parse_json_stream` for JSON arrays. The manual bracket-depth parser (`state`, `depth`, `in_string`) consumes elements individually without loading the full array.

**Verified:** `agent-test/large_uniform_500.json` (500 objects, 31818 bytes) converts to 12347 bytes (55.6% savings). The streaming parser handles nested objects (`parse_json_stream_nested_objects`) and mixed types (`parse_json_stream_mixed_types`).

**Suggestion:** For arrays >2 MiB, verify `max_input_bytes` (default 2 MiB in `ConversionOptions`). The structural depth guard (`exceeds_max_structural_depth`) prevents stack overflow before `sonic-rs` parsing.

## Supply chain / audit documentation

**Use case:** TOML/YAML configuration files, audit trails, compliance records.

**Approach:** Small structured files (`supply-chain/audits.toml`: 13 → 8 bytes, 38.5% savings) show high percentage savings despite small absolute sizes. Use `tooned index .` to scan entire repositories and rank savings opportunities.

**Verified:** Index scan on this repo found 231 files (183 classified). Top savings include `supply-chain/audits.toml` (38.5%), `rust-toolchain.toml` (10.3%), `rustfmt.toml` (9.5%).

**Suggestion:** Run `tooned index .` in CI to detect new high-savings files. Use `tooned stats --json` for machine-readable reports that can be posted to dashboards or PR comments.

## Binary format integration

**Use case:** MessagePack (`users_20.msgpack`: 47.5% savings) and CBOR (`products_20.cbor`: 48.6% savings) payloads.

**Approach:** The detection layer (`detect()` in `tooned-detect`) identifies binary formats from content. Conversion produces the same TOON output as JSON equivalents.

**Verified:** Both fixtures convert with identical savings percentages to their JSON counterparts. The `Msgpack` and `Cbor` doc types are fully supported (`tooned check` reports correctly).

**Suggestion:** For APIs using binary serialization (e.g., gRPC, WebSocket binary frames), consider converting to TOON at the agent layer before displaying results, reducing token costs without changing the underlying binary protocol.

## Metrics tracking (development)

**Use case:** Tracking token savings over time for optimization, cost analysis, or A/B testing.

**Approach:** The `tooned-metrics` crate (`store.rs`) defines `Metric` (token/byte variants), `MetricLedger`, and `TokenSavingsArgs`. The CLI surface (`metrics` subcommand) may vary by build; verify with `tooned --help`.

**Verified:** Metrics store source exists (`crates/tooned-metrics/src/store.rs`). The `.tooned/metrics.db` SQLite database records events. The binary in this environment does not expose `metrics` (verified via `tooned --help` output), suggesting either a minimal build or a feature-gated CLI surface.

**Suggestion:** Check `Cargo.toml` for feature flags controlling CLI surfaces. If metrics are needed, build with full features (`cargo build --all-features`) or access `.tooned/metrics.db` directly via SQLite queries.

## Hook integration (development)

**Use case:** Testing agent hooks without `additionalContext` tainting results.

**Approach:** Use `.devin/hooks.v1.json` (or equivalent per agent) with `PostToolUse` matcher. Confirm no `additionalContext` is emitted (`devin::run` source does not include it). Test replacement protocol with `updatedToolOutput` (Claude Code) or `continue: false` + `reason` (Codex).

**Verified:** Hook file uses `DEVIN_MATCHER` (`^exec$|^read$|...`). `hook doctor --json` reports hook installations. `hook status --claude-code --scope project` exits 2 (not installed at this scope — confirms agent-specific installation). Uninstall dry-run works (`--dry-run`).

**Suggestion:** For CI testing, install the hook temporarily (`tooned hook install --claude-code --scope project --dry-run` to verify path), run the agent with a controlled payload, and verify `hookSpecificOutput` contains only `updatedToolOutput` (no `additionalContext`). Log results with fixture name, prompt, and whether `SKU-1001` (or equivalent mismatch value) appears.

## Regression and validation tracking

**Finding:** `agent-test/complex/ecommerce_orders.json` and `agent-test/complex/sensor_readings.ndjson` now show `RoundTripMismatch` with the current encoder (`toon-format 0.5.0`, `toon-lsp 0.7.21`).

**Suggestion:** Before relying on these fixtures for production validation, compare current encoder output with previous build output (`git diff` on `Cargo.lock` or crate versions). If the fixtures themselves haven't changed (`agent-test/complex/ecommerce_orders.json` timestamp: Jul 17 16:26), the mismatch indicates either:
- A regression in `tooned-toon` encoding/decoding
- A stricter validation in `decode_toon`
- A change in default `ConversionOptions` (`dict_enabled`, `auto_margin`, `entropy_gate`)

**Action:** Run `tooned diff agent-test/complex/ecommerce_orders.json` and compare with earlier build. If the diff shows structural differences (not just byte differences), the encoding has changed. If the diff shows identical structure but `convertible: no`, the validation has become stricter.

## Edge case recommendations

Based on actual test results:

- **Empty arrays (`[]`)**: Not convertible (`NotSmallerEnough`). Do not expect savings for empty data structures.
- **Plain text (`plain.txt`)**: Not structured (`NotStructuredData`). Pipeline correctly passes through.
- **JSON5 (`sample_complex.json5`)**: Slightly larger in TOON form (`NotSmallerEnough`, -2.5%). Consider using standard JSON for small JSON5 payloads.
- **Deep nesting (adversarial)**: Intercepted by depth guard (`ParseError::TooDeep`). No crash, no incorrect encoding.
- **Binary formats (`msgpack`, `cbor`)**: Fully supported. Savings match JSON equivalents.
- **Concurrent index access**: Safe (`WAL` mode + atomic rename). Use `index sync` for incremental updates.

## Development workflow suggestions

1. **Test pipeline verification first:** `tooned check` on fixtures verifies conversion behavior. `tooned pipe` verifies streaming output size. `tooned diff` verifies round-trip fidelity (for JSON inputs).
2. **Document actual results, not simulated:** All evidence in `docs/agents/toon-evidence.md` comes from actual binary executions.
3. **Track regressions explicitly:** Note fixtures that change behavior between builds (`ecommerce_orders.json`, `sensor_readings.ndjson`).
4. **Rate-limit external model calls:** Agent CLI tests with `swe-1.7-max`, `glm-5.2` (high) must be staggered. Log each result individually.
5. **Avoid `additionalContext` for isolation tests:** Confirm hook output does not contain it (`devin::run` source confirms absence; `.devin/hooks.v1.json` uses replacement protocol).
6. **Use executable test scripts:** `scripts/test_evidence.sh` documents fixtures, protocol, conversion results, and instructions for staggered runs.
