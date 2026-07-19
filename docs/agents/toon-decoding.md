# TOON decoding across formats

A model given a TOON tool result extracts values as if it were reading a table. It sees the header `[20]{sku,name,price,qty,category}:` and the rows, then answers questions like "what is the SKU of the first product?" with `SKU-1001`. No `toon → json` conversion runs inside the agent; the model maps the header/row structure to the question directly.

This works because TOON is explicit about structure: field names are declared once, rows are comma-separated, and array lengths are in the header. The same pattern appears in CSV and Markdown tables, which models already see in tool output, so they can map the header/row structure to the question without learning a new parser.

The mismatch test in [`toon-evidence.md`](toon-evidence.md) isolates this behavior by injecting the TOON of `products_20.json` while reading `users_20.json` (which has no `sku` field). The model still returned `SKU-1001`.

## When `tooned` converts a payload

`tooned` only emits TOON when the encoding is smaller than the compact JSON representation and `decode(encode(x)) == x`. The current conversion results for the test fixtures are in [`toon-evidence.md`](toon-evidence.md) (simple cross-format) and [`research/toon-format-research.md`](research/toon-format-research.md) (complex fixtures).

## Reading is easier than writing

`tooned` asks the model to *read* TOON, not *write* it. Generating valid TOON accurately is harder than reading it.

For the test results and the research context, see [`toon-evidence.md`](toon-evidence.md). For the conversion pipeline details, see [`research/toon-format-research.md`](research/toon-format-research.md).
