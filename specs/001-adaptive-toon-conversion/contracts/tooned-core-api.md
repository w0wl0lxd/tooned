# Contract: `tooned-core` public API

The single entrypoint every integration surface (CLI, both hooks, MCP server) calls
into — no surface re-implements detection/conversion logic itself (constitution
Principle V).

```rust
/// Never returns Err for payload-driven failure (malformed/oversized/ambiguous
/// input) — those always resolve to Ok(Conversion::Passthrough { .. }).
/// Err is reserved for caller misuse (invalid options).
pub fn maybe_tooned(
    input: &[u8],
    opts: &ConversionOptions,
) -> Result<Conversion, ToonedError>;

/// Dry-run: same detection + shape classification as maybe_tooned, but never
/// computes or returns TOON text. Backs `tooned check`.
pub fn inspect(input: &[u8], opts: &ConversionOptions) -> InspectReport;

/// Decode a TOON document back to a structured value. Used by `tooned convert
/// --to json` and the MCP `tooned_decode` tool.
pub fn decode_toon(text: &str) -> Result<serde_json::Value, ToonedError>;
```

## Pre/postconditions

- **Precondition**: `input.len()` MAY exceed `opts.max_input_bytes`; `maybe_tooned`
  MUST check this before attempting to parse (FR-006) and return
  `Passthrough { reason: InputTooLarge }` without invoking any parser.
- **Postcondition (never-regression)**: for every `Conversion::Toon` returned,
  `report.toon_bytes < report.json_bytes`. Property-tested (constitution Principle IV).
- **Postcondition (round-trip fidelity)**: for every `Conversion::Toon` returned,
  `decode_toon(&text)` MUST succeed and be structurally equal (after normalizing to
  compact JSON) to the value that was encoded. Property-tested.
- **Postcondition (no panics)**: `maybe_tooned` and `inspect` MUST NOT panic for any
  `&[u8]` input, including invalid UTF-8, truncated multi-byte sequences, and
  adversarially deep nesting. Fuzz/property-tested, not just example-tested.
- **Latency**: `maybe_tooned` on a ~100 KiB well-formed uniform-array payload MUST
  complete in low single-digit milliseconds (constitution Technology Constraints);
  enforced by an `--ignored` criterion-backed guardrail test, not asserted at every
  call site.
