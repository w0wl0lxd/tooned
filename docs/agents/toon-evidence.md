# Evidence: the model reads and reasons over TOON

This document supports the README claim ([`toon-example.md`](toon-example.md)): an agent was run with `tooned` enabled, the tool output was rewritten into TOON, and the model still reasoned about the data as if it had the original JSON.

The evidence was collected from a live run using an agent protocol that replaces the native tool result with TOON (`updatedToolOutput` for Claude Code/OpenCode/Kilo/Pi, `continue: false` + `reason` feedback for Codex). With these protocols the model receives only the TOON; the original JSON is not in that context item. `additionalContext`-only agents (Devin, Droid) cannot deliver a TOON-only result in `PostToolUse`, so `tooned` does not emit `additionalContext` for them; use command-level wrapping (`tooned wrap -- <cmd>` or `... | tooned pipe`) when TOON-only output is required.

> **Caveat.** A model's visible output cannot by itself prove which context bytes drove the answer. The first observations below are consistent with the model reading either the original JSON or the TOON. Only the mismatch test in Finding 3 isolates the TOON result. Treat the first findings as supporting evidence that the hook ran and the model received coherent structured data; treat Finding 3 as the strongest evidence that the model consulted the TOON.

## Apparatus

- A `PostToolUse` hook or wrapped command that replaces the native tool result with a TOON encoding when TOON is smaller and round-trips. The original tool output is replaced; the model sees only TOON.
- A second configuration that replaces the tool result with the TOON of a *different* file, to isolate whether the model reads the TOON result.

---

## Finding 1 — The hook runs and the model summarizes the data

A `read` of `agent-test/users_20.json` produced a natural-language summary of the 20 user objects. The visible output was the TOON text because the tool result was replaced, not augmented with `additionalContext`.

This supports: the conversion pipeline ran and the model received a coherent structured view of the data.

---

## Finding 2 — Output alone cannot prove the model used the TOON

The summary in Finding 1 is compatible with either the original JSON or a TOON result that contained equivalent data, but it does not prove the model used the TOON rather than the original source. This honest null result motivated the controlled mismatch test in Finding 3.

---

## Finding 3 — Mismatch test: the model read the TOON

**Setup.** The tool result was replaced with the TOON encoding of `agent-test/products_20.json` while the agent `read` `agent-test/users_20.json` (which has no `sku` field). The prompt asked for the SKU of the first product.

**Result.**

```text
The SKU of the first product is SKU-1001.
```

That value exists only in the replaced TOON result, never in the original `read` output.

**Supports (not proves):** `SKU-1001` was available only in the TOON result, so the model must have consulted that result. No external decoder ran. The test did not include randomized values, repeated trials, or captured raw context, so we describe this as strong supporting evidence rather than a formal proof.

---

## Finding 4 — Exact-content requests return TOON

When the prompt asked to print the file unchanged, the model returned the TOON text. Because the agent protocol replaced the tool result with TOON, the original JSON is no longer in that context item.

This supports: the model can read the TOON result directly; TOON is not just an appended annotation.

---

## Summary

| # | Claim | Evidence |
|---|-------|----------|
| 1 | Hook runs; TOON is in the tool result | model produced a coherent summary; replacement protocol was used |
| 2 | Output alone can't prove TOON usage | the first summary could have come from equivalent original or TOON data |
| 3 | Model read the TOON result | mismatch result `SKU-1001` from a source only in the TOON result |
| 4 | Replacement is real, not augmentation | exact-output request returned the TOON text, not JSON |

**Conclusion (scoped to the tested runs).** In the observed runs the model could answer structured questions about the data without needing the original JSON syntax, when TOON was the only source of the requested field. This is offered as supporting evidence of `tooned`'s design intent (smaller context, same comprehension), not as a universal claim that every model comprehends TOON as accurately as JSON.

## More

- [`toon-example.md`](toon-example.md) — the worked example.
- [`toon-context-proof.md`](toon-context-proof.md) — backend flow and the mismatch proof.
- [`toon-decoding.md`](toon-decoding.md) — cross-format decoding and research context.
