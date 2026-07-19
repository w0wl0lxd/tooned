# TOON decoding

A model given a TOON tool result extracts values by reading the header and rows directly. It does not need a `toon → json` conversion step; it maps the header/row structure to the question as it would for CSV or Markdown tables.

The mismatch test isolates this behavior: when `users_20.json` (no `sku` field) is read with the TOON of `products_20.json` injected, the prompt `"SKU of the first product"` returns `SKU-1001`, which only exists in the injected TOON. Without `additionalContext` tainting the result, this confirms the model consulted the TOON.

## When `tooned` converts

`tooned` only emits TOON when the encoding is smaller than compact JSON and `decode(encode(x)) == x` exactly. The current fixtures (`agent-test/` and `agent-test/complex/`) cover uniform arrays (convert), nested objects (convert or within margin), array-of-arrays (JSON stays smaller), and non-uniform arrays (JSON stays smaller). See [`toon-evidence.md`](toon-evidence.md) for the fixture table.

Reading TOON is easier than writing it. `tooned` only asks the model to read; generation of valid TOON is a separate, harder task.
