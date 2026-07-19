# TOON Model-Side Decoding — A Live Cross-Format Test

This document shows that a Large Language Model, given a TOON-encoded tool result from a `PostToolUse` hook or wrapped command, can decode the TOON into the underlying structured data model and answer as if it had read the original JSON, with no external decoder involved.

`tooned` does not use `additionalContext`: for agents that only support `additionalContext` in `PostToolUse` (Devin, Droid), the original JSON would remain in context alongside the TOON, inflating total token count. Those agents require command-level wrapping (`tooned wrap -- <cmd>` or `... | tooned pipe`) for TOON-only output. The examples below assume an agent protocol that replaces the native tool result with TOON.

## The core observation

When `tooned` feeds TOON to the model as the tool result like this:

```toon
[20]{sku,name,price,qty,category}:
  SKU-1001,"Product 1",11.49,7,home
  ...
```

the model does **not** echo the TOON syntax back. Instead it extracts the requested field and replies in natural language, e.g.:

> The SKU of the first product is `SKU-1001`.

It is doing an implicit, learned decode: it sees the header `{sku,name,...}`, maps it to a table schema, picks the first row, and returns the `sku` value. No `toon → json` conversion step runs inside the agent.

## Why this is expected

TOON was deliberately designed to be human-readable and structurally explicit. The header/row convention is well represented in pretraining data (CSV, Markdown tables, YAML lists, SQL result sets, logs), so the model can infer the schema without a bespoke parser.

## The decisive mismatch test

1. The agent `read`s `agent-test/users_20.json` (JSON array of user objects, no `sku` field).
2. The tool result is replaced with the TOON encoding of `agent-test/products_20.json`.
3. Prompt: `read the file users_20.json and tell me the SKU of the first product`.
4. Model answered: `The SKU of the first product is SKU-1001.`

Because `users_20.json` contains no `sku` field, the answer strongly supports that it came from the TOON tool result. This is consistent with the model parsing the TOON header and first row and returning the `sku` value.

## Observed transcript

```text
> read agent-test/users_20.json
Done. I read agent-test/users_20.json — it's a JSON array of 20 user objects with id, name, email, active, and role fields.

> read the file users_20.json and tell me the SKU of the first product
The SKU of the first product is SKU-1001.
```

## Is this a big thing?

It is a useful result in practice, but it is not a new discovery about how models work. LLMs can already read and reason over alternative structured-data serializations as long as the logical structure is preserved. The specific demonstration here is valuable because it shows that:

1. A `PostToolUse` hook or wrapped command can feed TOON to the model as the tool result without breaking normal behavior.
2. The model can use that TOON result to answer questions exactly as it would from JSON.
3. When the agent protocol replaces the tool result, the original JSON is not in that context item; exact-copy and verbatim-output requests return the TOON text.

In other words, `tooned` does not have to ship a decoder or teach the model TOON. The model already knows how to read it from its pretraining.

## Reading is easier than writing

These tests measure **comprehension** (reading TOON and answering). Getting a model to **generate** valid TOON accurately is harder. `tooned` is designed around this asymmetry: it only asks the model to *read* TOON, never to *write* it.

## More

- Backend flow + diagrams: [`toon-context-proof.md`](toon-context-proof.md)
- Cross-format decoding: [`toon-decoding.md`](toon-decoding.md)
