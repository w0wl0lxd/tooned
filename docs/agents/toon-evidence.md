# Evidence: the model reads and reasons over TOON

This document supports the README claim ([I tested this](toon-example.md)):
an agent was run with `tooned` enabled and its data rewritten into TOON, yet it
still reasoned about the data as if it were the original JSON.

Each finding is backed by the model's **own internal reasoning** (its thinking
trace), captured live. The traces are reproduced here in a **genericized**
form — no product/agent names, file paths, or personal identifiers — so the
evidence is reviewable without leaking environment specifics.

## Apparatus

- A `PostToolUse` hook that converts the tool's JSON output into TOON and
  injects it as `additionalContext`. The original tool output is preserved.
- A second hook config that injects the TOON of a *different* file as
  `additionalContext`, to isolate whether the model is reading the TOON.

---

## Finding 1 — The hook runs and the model summarizes from context

**Observation.** A `read` of a JSON array of user objects produced a
natural-language summary. The answer looked like normal JSON because the hook
adds `additionalContext` rather than replacing the visible tool output.

**Model's reasoning (genericized):**

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

**Model's reasoning (genericized):**

> The output alone can't prove the model read TOON. The original tool output
> is still in the result; `additionalContext` is extra. It could have
> summarized from either. To prove it reads TOON, we need a case where the
> answer can only come from TOON.

**Supports:** this honest null result motivated the decisive test in Finding 3.
The experiment was designed to *disprove* the hypothesis, not just confirm it.

---

## Finding 3 — The mismatch proof: the model decoded TOON on its own

**Setup.** The hook injected, as `additionalContext`, the TOON of a *products*
file while the agent `read` a *users* file (no `sku` field). The prompt asked
for the SKU of the first product.

**Result.**

> The SKU of the first product is `SKU-1001`.

That value exists **only** in the TOON `additionalContext`, never in the
original `read` output.

**Model's reasoning (genericized):**

> The model answered with the SKU from the products TOON `additionalContext`,
> while the original `read` output was the users file. This proves the agent
> injects `additionalContext`, the model reads and uses it, and it can parse
> TOON and extract structured information from it. The earlier exact-content
> test used the original output; this analysis question used the TOON context
> because the original file had no SKUs. Strong proof the model reads TOON
> directly.

**Supports:** the model performed the decode internally — no external decoder
ran. The syntax changed; the data model did not.

---

## Finding 4 — Fidelity preserved for exact-content requests

**Observation.** When the prompt asked to print the file *unchanged*, the model
returned the original JSON. The hook keeps the original tool output next to the
TOON context, so the model can fall back to it.

**Model's reasoning (genericized):**

> For exact-raw-output prompts the model uses the original tool output; for
> analysis it can use the TOON context. Both are present, and it chooses
> appropriately. No fidelity is lost.

**Supports:** TOON shrinks context for convertible payloads without ever
preventing exact reproduction of the original when asked.

---

## Summary

| # | Claim | Evidence |
|---|---|---|
| 1 | Hook runs; TOON is in context | reasoning trace + hook invocation |
| 2 | Output alone can't prove usage | reasoning trace (null result) |
| 3 | Model decodes TOON internally | mismatch result `SKU-1001` + trace |
| 4 | Original preserved on demand | reasoning trace + exact-output result |

**Conclusion.** The model does not need the original JSON syntax to understand
the data. Given TOON, it reads and reasons over the structure directly — the
whole point of `tooned`: smaller context with no loss of comprehension.

## More

- Backend flow diagrams: [`toon-hook-flow.md`](toon-hook-flow.md)
- Cross-format decoding test + research: [`toon-decoding.md`](toon-decoding.md)
