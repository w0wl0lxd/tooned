# Evidence: the model reads and reasons over TOON

This document supports the README claim ([`toon-example.md`](toon-example.md)): an agent was run with `tooned` enabled, the tool output was rewritten into TOON, and the model still reasoned about the data as if it had the original JSON.

The evidence was collected from a live `PostToolUse` hook run. The protocol surfaces TOON as `additionalContext` while the original tool output remains visible, so the model receives both. No model-specific names, file paths, or identifiers are included below.

> **Caveat.** A model's visible output cannot by itself prove which context bytes drove the answer. The first observations below are consistent with the model reading either the original JSON or the TOON. Only the mismatch test in Finding 3 isolates the TOON context. Treat the first findings as supporting evidence that the hook ran and the model received coherent structured data; treat Finding 3 as the strongest evidence that the model consulted the TOON.

## Apparatus

- A `PostToolUse` hook that converts the tool's JSON output into TOON and surfaces it as `additionalContext`. The original tool output is preserved.
- A second hook configuration that injects the TOON of a *different* file as `additionalContext`, to isolate whether the model reads the injected TOON.

---

## Finding 1 — The hook runs and the model summarizes the data

A `read` of `agent-test/users_20.json` produced a natural-language summary of the 20 user objects. The visible output looked like normal JSON because the hook adds `additionalContext` rather than replacing the visible tool output.

This supports: the conversion pipeline ran, the TOON `additionalContext` was generated, and the model received a coherent structured view of the data.

---

## Finding 2 — Output alone cannot prove the model used the TOON

The summary in Finding 1 could have come from either the original JSON or the TOON `additionalContext`; both were in context. This honest null result motivated the controlled mismatch test in Finding 3.

---

## Finding 3 — Mismatch test: the model read the TOON

**Setup.** The hook injected, as `additionalContext`, the TOON encoding of `agent-test/products_20.json` while the agent `read` `agent-test/users_20.json` (which has no `sku` field). The prompt asked for the SKU of the first product.

**Result.**

```text
The SKU of the first product is SKU-1001.
```

That value exists only in the injected TOON `additionalContext`, never in the original `read` output.

**Supports (not proves):** `SKU-1001` was available only in the injected TOON context, so the model must have consulted that context. No external decoder ran. The test did not include randomized values, repeated trials, or captured raw context, so we describe this as strong supporting evidence rather than a formal proof.

---

## Finding 4 — Fidelity preserved for exact-content requests

When the prompt asked to print the file unchanged, the model returned the original JSON. Under the `additionalContext` protocol the original tool output is kept alongside the TOON, so the model can fall back to it.

This supports: TOON can shrink context for convertible payloads without preventing exact reproduction of the original when asked.

---

## Summary

| # | Claim | Evidence |
|---|-------|----------|
| 1 | Hook runs; TOON is in context | model produced a coherent summary; hook command was invoked |
| 2 | Output alone can't prove TOON usage | the first summary could have come from either context |
| 3 | Model read the TOON context | mismatch result `SKU-1001` from a source only in the TOON context |
| 4 | Original preserved on demand | exact-output request returned the original JSON |

**Conclusion (scoped to the tested runs).** In the observed runs the model could answer structured questions about the data without needing the original JSON syntax, when TOON was the only source of the requested field. This is offered as supporting evidence of `tooned`'s design intent (smaller context, same comprehension), not as a universal claim that every model comprehends TOON as accurately as JSON.

## More

- Backend flow diagrams: [`toon-context-proof.md`](toon-context-proof.md)
- Cross-format decoding test + research: [`toon-decoding.md`](toon-decoding.md)
