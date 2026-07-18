// SPDX-License-Identifier: AGPL-3.0-only

//! Cache-aware cost estimation (F3/F10).
//!
//! Grounded in the July-2026 cost-composition study (arXiv 2607.12161): across
//! 2,908 billed Claude Code runs, prompt-cache traffic accounted for ~87% of
//! reconstructed cost, and raw tool-output token reduction did *not* reliably
//! predict billed-cost reduction. A single "tokens * input price" figure
//! therefore misestimates real spend; this module splits token quantities into
//! the four provider-priced buckets (input / cache-write / cache-read / output)
//! and prices each at its own July-2026 list rate.
//!
//! TOON is a *lossless* structural re-encoding, so unlike the lossy/extractive
//! compressors the study shows failing, its per-step token reduction translates
//! to real savings without lengthening trajectories. The dollar figures here
//! are planning estimates (constants, never fetched): real invoices depend on
//! provider-side cache composition the local ledger cannot see, so aggregation
//! reports a conservative, token-proportional lower bound. Callers needing
//! exact invoicing can supply their own [`ModelPricing`].

use tooned_types::TokenizerProfile;

/// Which provider-priced bucket a token quantity falls into. Providers charge
/// differently across these -- cache reads are dramatically cheaper than cache
/// writes or fresh input -- which is exactly why a flat token count misprices
/// cost (arXiv 2607.12161).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Fresh input tokens (never cached).
    Input,
    /// Cache-write tokens (writing a prefix to the KV cache).
    CacheWrite,
    /// Cache-read tokens (reusing a cached prefix).
    CacheRead,
    /// Generated output tokens.
    Output,
}

/// Per-1M-token USD list pricing for one model family. Values are
/// representative July-2026 OpenAI list prices, treated as planning estimates
/// (not invoices): real spend depends on provider-side cache composition,
/// volume discounts, and usage tier. The structure
/// (`cache_read < cache_write <= input < output`) matches OpenAI's prompt
/// -caching price schedule; the cache-read discount is the lever the
/// 2607.12161 study identifies as dominating cost.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    /// Fresh input tokens (never cached).
    pub input: f64,
    /// Cache-write tokens (writing a prefix to the KV cache).
    pub cache_write: f64,
    /// Cache-read tokens (reusing a cached prefix).
    pub cache_read: f64,
    /// Generated output tokens.
    pub output: f64,
}

impl ModelPricing {
    /// GPT-4o / o-series (`o200k_base`) representative July-2026 list prices.
    pub const O200K: Self =
        Self { input: 2.50, cache_write: 2.50, cache_read: 0.625, output: 10.00 };
    /// GPT-4 / GPT-3.5 era (`cl100k_base`) representative July-2026 list prices.
    pub const CL100K: Self =
        Self { input: 5.00, cache_write: 5.00, cache_read: 1.25, output: 15.00 };
    /// Heuristic-profile fallback: a neutral mid-range estimate used when the
    /// model family is unknown.
    pub const HEURISTIC: Self =
        Self { input: 3.00, cache_write: 3.00, cache_read: 0.75, output: 12.00 };

    /// Price per 1M tokens for `kind`.
    #[must_use]
    pub fn per_million(self, kind: TokenKind) -> f64 {
        match kind {
            TokenKind::Input => self.input,
            TokenKind::CacheWrite => self.cache_write,
            TokenKind::CacheRead => self.cache_read,
            TokenKind::Output => self.output,
        }
    }
}

/// Resolve the default pricing for a tokenizer profile. A
/// [`Named`](TokenizerProfile::Named) model is resolved through the same
/// family mapping the token counter uses, so `pricing_for` and `count_tokens`
/// always agree on a model's family; unknown names fall back to the heuristic
/// estimate.
#[must_use]
pub fn pricing_for(profile: &TokenizerProfile) -> ModelPricing {
    let resolved = match profile {
        TokenizerProfile::Named(name) => crate::resolve_model(name),
        p => p.clone(),
    };
    match resolved {
        TokenizerProfile::Cl100k => ModelPricing::CL100K,
        TokenizerProfile::O200k => ModelPricing::O200K,
        _ => ModelPricing::HEURISTIC,
    }
}

/// Estimate the USD cost of `tokens` under `profile` at the `kind` price.
/// `tokens` is the raw count (not per-million); the result is dollars.
#[must_use]
pub fn estimate_cost_usd(tokens: u64, profile: &TokenizerProfile, kind: TokenKind) -> f64 {
    let per_million = pricing_for(profile).per_million(kind);
    (tokens as f64 / 1_000_000.0) * per_million
}

/// USD saved by reducing a token count from `json_tokens` to `toon_tokens`,
/// priced at the `kind` rate. Never negative: if `toon_tokens` exceeds
/// `json_tokens` the result is clamped to 0 (a re-encoding that grew the
/// output never saves money).
#[must_use]
pub fn cost_savings_usd(
    json_tokens: u64,
    toon_tokens: u64,
    profile: &TokenizerProfile,
    kind: TokenKind,
) -> f64 {
    let saved = json_tokens.saturating_sub(toon_tokens);
    estimate_cost_usd(saved, profile, kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn pricing_eq(a: ModelPricing, b: ModelPricing) -> bool {
        approx(a.input, b.input)
            && approx(a.cache_write, b.cache_write)
            && approx(a.cache_read, b.cache_read)
            && approx(a.output, b.output)
    }

    #[test]
    fn cache_read_is_cheaper_than_input() {
        let p = ModelPricing::O200K;
        assert!(p.cache_read < p.input, "cache read must be cheaper than input");
        assert!(p.cache_read < p.cache_write);
    }

    #[test]
    fn estimate_cost_scales_linearly() {
        let prof = TokenizerProfile::O200k;
        let one_m = estimate_cost_usd(1_000_000, &prof, TokenKind::Input);
        let two_m = estimate_cost_usd(2_000_000, &prof, TokenKind::Input);
        assert!(approx(two_m, 2.0 * one_m));
        assert!(one_m > 0.0);
    }

    #[test]
    fn cost_savings_is_nonnegative_and_zero_safe() {
        let prof = TokenizerProfile::Cl100k;
        assert!(approx(cost_savings_usd(0, 0, &prof, TokenKind::Input), 0.0));
        assert!(approx(cost_savings_usd(100, 200, &prof, TokenKind::Input), 0.0));
        assert!(cost_savings_usd(1_000_000, 500_000, &prof, TokenKind::Input) > 0.0);
    }

    #[test]
    fn pricing_for_maps_profiles() {
        assert!(pricing_eq(pricing_for(&TokenizerProfile::O200k), ModelPricing::O200K));
        assert!(pricing_eq(pricing_for(&TokenizerProfile::Cl100k), ModelPricing::CL100K));
        assert!(pricing_eq(pricing_for(&TokenizerProfile::Heuristic), ModelPricing::HEURISTIC));
        assert!(pricing_eq(
            pricing_for(&TokenizerProfile::Named("gpt-4o".to_string())),
            ModelPricing::O200K
        ));
    }

    #[test]
    fn output_priced_higher_than_input() {
        let o = ModelPricing::O200K;
        let c = ModelPricing::CL100K;
        assert!(o.output > o.input);
        assert!(c.output > c.input);
    }

    #[test]
    fn custom_pricing_overrides_defaults() {
        let custom = ModelPricing { input: 1.0, cache_write: 1.0, cache_read: 0.1, output: 4.0 };
        assert!(approx(custom.per_million(TokenKind::Input), 1.0));
        assert!(approx(custom.per_million(TokenKind::CacheRead), 0.1));
    }
}
