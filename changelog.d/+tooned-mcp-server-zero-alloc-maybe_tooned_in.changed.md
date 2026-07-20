`tooned mcp serve` now converts content using `tooned_core::maybe_tooned_in` with a per-request output buffer, avoiding a separate TOON `String` allocation on each `tools/toon` call.
