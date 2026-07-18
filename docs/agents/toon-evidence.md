# Evidence: the model reads and reasons over TOON

This document supports the README claim ([I tested this](toon-example.md)):
an agent was run with `tooned` enabled and its data rewritten into TOON, yet it
still reasoned about the data as if it were the original JSON.

The evidence below was produced with the **Devin** protocol, which surfaces TOON
as `additionalContext` while preserving the original tool output. Each finding
is paired with the **model's own explanation** (its thinking trace), captured
live. Those traces are reproduced here in a **genericized** form — no
product/agent names, file paths, or personal identifiers — so the narrative is
reviewable without leaking environment specifics.

> **How to read these traces.** A model's self-reported thinking is a
> post-hoc explanation, not a verified causal account of which context bytes
> drove the answer. Treat them as qualitative observations that *support* the
> claims below, not as proof. The reproducible facts are the prompts, the hook
> payloads, and the model's final outputs.

## Apparatus

- A `PostToolUse` hook that converts the tool's JSON output into TOON and
  surfaces it to the model. For Devin/Codex/Droid this is `additionalContext`
  (original preserved); for Claude Code/OpenCode/Kilo/Pi it is
  `updatedToolOutput` (TOON replaces the output).
- A second hook config that injects the TOON of a *different* file as
  `additionalContext`, to isolate whether the model is reading the TOON.

---

## Finding 1 — The hook runs and the model summarizes from context

**Observation.** A `read` of a JSON array of user objects produced a
natural-language summary. The answer looked like normal JSON because the hook
adds `additionalContext` rather than replacing the visible tool output.

**Model's explanation (genericized):**

> The `read` produced a JSON summary, not raw TOON. That doesn't mean the hook
> failed — it means the model used the original tool output for the visible
> answer. The `additionalContext` is injected into the model's context for
> token savings, so the savings happen in the context it actually reads. We
> confirmed the hook command was invoked, so the TOON was present.

**Supports:** the conversion happens behind the scenes; the model receives the
TOON context whether or not the visible answer shows it.

---

## Finding 2 — Output alone can't yet confirm the model *used* the TOON

**Observation.** The first summaries were consistent with the model reading
either the original JSON or the TOON — both were in context.

**Model's explanation (genericized):**

> The output alone can't prove the model read TOON. The original tool output
> is still in the result; `additionalContext` is extra. It could have
> summarized from either. To prove it reads TOON, we need a case where the
> answer can only come from TOON.

**Supports:** this honest null result motivated the test in Finding 3, which
was designed to *disprove* the hypothesis, not just confirm it.

---

## Finding 3 — The mismatch test: the model read the TOON

**Setup.** The hook injected, as `additionalContext`, the TOON of a *products*
file while the agent `read` a *users* file (no `sku` field). The prompt asked
for the SKU of the first product.

**Result.**

> The SKU of the first product is `SKU-1001`.

That value exists **only** in the TOON `additionalContext`, never in the
original `read` output.

**Model's explanation (genericized):**

> The model answered with the SKU from the products TOON `additionalContext`,
> while the original `read` output was the users file. This supports that the
> agent injects `additionalContext`, the model reads and uses it, and it can
> parse TOON and extract structured information from it. The earlier
> exact-content test used the original output; this analysis question used the
> TOON context because the original file had no SKUs.

**Supports (not proves):** `SKU-1001` was available *only* in the injected TOON
context, so the model must have consulted that context to produce it. No
external decoder ran. The syntax changed; the data model did not. We describe
this as strong supporting evidence rather than a formal proof, because the test
did not include controls such as derived multi-field computations, randomized
values, repeated trials, or captured raw outputs.

---

## Finding 4 — Fidelity preserved for exact-content requests (Codex/Devin/Droid)

**Observation.** When the prompt asked to print the file *unchanged*, the model
returned the original JSON. Under the Devin/Codex/Droid protocols the hook
keeps the original tool output next to the TOON context, so the model can fall
back to it.

**Model's explanation (genericized):**

> For exact-raw-output prompts the model uses the original tool output; for
> analysis it can use the TOON context. Both are present, and it chooses
> appropriately.

**Supports:** TOON shrinks context for convertible payloads without preventing
exact reproduction of the original when asked — for the protocols that preserve
the original output.

---

## Summary

| # | Claim | Evidence |
|---|---|---|
| 1 | Hook runs; TOON is in context | model explanation + hook invocation |
| 2 | Output alone can't prove usage | model explanation (null result) |
| 3 | Model read the TOON context | mismatch result `SKU-1001` + explanation |
| 4 | Original preserved on demand (Devin/Codex/Droid) | model explanation + exact-output result |

**Conclusion (scoped to the tested runs).** In the observed runs, the model did
not need the original JSON syntax to answer structured questions about the data
— given TOON, it read and reasoned over the structure directly. This held for
the model and configuration used here; it is offered as supporting evidence of
`tooned`'s design intent (smaller context, same comprehension), not a universal
proof that every model comprehends TOON as accurately as JSON.

## More

- Backend flow diagrams: [`toon-hook-flow.md`](toon-hook-flow.md)
- Cross-format decoding test + research: [`toon-decoding.md`](toon-decoding.md)
