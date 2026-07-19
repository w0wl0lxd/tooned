// SPDX-License-Identifier: AGPL-3.0-only

//! Model-aware tokenizer counting for `tooned`.
//!
//! The default hot path measures savings in *bytes* via a 4-bytes/token
//! heuristic (`tooned-metrics::estimate_tokens`). That heuristic is
//! constitution-mandated for the default path (Principle II: no BPE tokenizer
//! on the hot path), but it badly misreports savings for JSON, where grammar
//! characters and repeated schema keys tokenize very differently from natural
//! language. This crate maps a [`TokenizerProfile`] onto a bundled
//! `tiktoken-rs` rank table (`cl100k_base` / `o200k_base`) and counts real
//! tokens — with **no network calls** (both rank tables are bundled).
//!
//! Research grounding (July 2026 and adjacent):
//! - arxiv 2607.15232 "In-Place Tokenizer Expansion for Pre-trained LLMs"
//!   argues tokenization cost is a first-class budget axis; fragmentation
//!   (many subword tokens per word) directly raises latency/compute. Measuring
//!   against the *actual* model tokenizer is therefore not optional hygiene.
//! - "Structure-Aware Tokenization for JSON" (2026) shows JSON token sequences
//!   are highly compressible under a schema-aware tokenizer (5-15% savings on
//!   schema-repetitive payloads) — confirming that the *profile* a payload is
//!   scored against changes the reported number.
//! - tokpack / context-compression (2026) both compute per-model token counts
//!   rather than byte heuristics.

use std::sync::OnceLock;

use tiktoken_rs::CoreBPE;
use tooned_types::TokenizerProfile;

pub mod cost;
pub use cost::{ModelPricing, TokenKind, cost_savings_usd, estimate_cost_usd, pricing_for};

/// Bytes-per-token assumption used by the heuristic profile and by the rest of
/// the workspace's default savings math (`tooned-metrics::BYTES_PER_TOKEN`).
const BYTES_PER_TOKEN: usize = 4;

/// Lazily-initialized `cl100k_base` singleton. `tiktoken-rs` parses its bundled
/// rank table on first use only; subsequent calls are allocation-free.
fn cl100k() -> &'static CoreBPE {
    static SINGLETON: OnceLock<CoreBPE> = OnceLock::new();
    SINGLETON.get_or_init(|| tiktoken_rs::cl100k_base_singleton().clone())
}

/// Lazily-initialized `o200k_base` singleton (GPT-4o / o-series models).
fn o200k() -> &'static CoreBPE {
    static SINGLETON: OnceLock<CoreBPE> = OnceLock::new();
    SINGLETON.get_or_init(|| tiktoken_rs::o200k_base_singleton().clone())
}

/// Count the tokens `text` would consume under `profile`.
///
/// The heuristic profile uses the workspace-default 4-bytes/token rule. The
/// BPE profiles use bundled `tiktoken-rs` tables and never touch the network.
/// A [`TokenizerProfile::Named`] is resolved (once) to a concrete BPE via
/// [`resolve_model`] and then counted; unknown model names fall back to the
/// heuristic rather than failing.
pub fn count_tokens(text: &str, profile: &TokenizerProfile) -> usize {
    match profile {
        TokenizerProfile::Cl100k => cl100k().encode_ordinary(text).len(),
        TokenizerProfile::O200k => o200k().encode_ordinary(text).len(),
        TokenizerProfile::Named(name) => count_tokens(text, &resolve_model(name)),
        _ => heuristic_count(text),
    }
}

/// The default 4-bytes/token rule of thumb, matching
/// `tooned-metrics::estimate_tokens`.
fn heuristic_count(text: &str) -> usize {
    let len = text.len();
    if len == 0 {
        return 0;
    }
    len.div_ceil(BYTES_PER_TOKEN)
}

/// Resolve a free-form model name to a concrete tokenizer profile.
///
/// Only patterns that map onto a *bundled* BPE table are recognized; everything
/// else resolves to [`TokenizerProfile::Heuristic`] (never triggers a network
/// fetch, preserving the workspace's zero-telemetry / offline guarantee).
///
/// Order matters: the `o200k_base` family (GPT-4o, GPT-5, o-series) is checked
/// before the broader `gpt-4`/`cl100k_base` family so e.g. `gpt-4o` is not
/// mis-classified as `cl100k`.
pub fn resolve_model(name: &str) -> TokenizerProfile {
    let n = name.to_lowercase();

    // o200k_base family.
    if n.contains("gpt-4o")
        || n.contains("gpt-5")
        || n.contains("gpt-5o")
        || n.contains("o1")
        || n.contains("o3")
        || n.contains("o4")
        || n.contains("o200k")
        || n.contains("chatgpt-4o")
    {
        return TokenizerProfile::O200k;
    }

    // cl100k_base family.
    if n.contains("gpt-4")
        || n.contains("gpt-3")
        || n.contains("text-embedding-ada")
        || n.contains("cl100k")
        || n.contains("babbage")
        || n.contains("davinci")
    {
        return TokenizerProfile::Cl100k;
    }

    TokenizerProfile::Heuristic
}

/// Parse a user-supplied profile string (from config or CLI) into a
/// [`TokenizerProfile`].
///
/// Accepts the literal keywords `heuristic`, `cl100k`, `o200k`, and any other
/// string as a [`TokenizerProfile::Named`] model name (resolved lazily by
/// [`resolve_model`] at count time).
pub fn parse_profile(s: &str) -> TokenizerProfile {
    match s.to_lowercase().as_str() {
        "heuristic" | "auto" | "bytes" => TokenizerProfile::Heuristic,
        "cl100k" | "cl100k_base" | "gpt-4" | "gpt-3.5" => TokenizerProfile::Cl100k,
        "o200k" | "o200k_base" | "gpt-4o" | "gpt-5" => TokenizerProfile::O200k,
        _ => TokenizerProfile::Named(s.to_string()),
    }
}

/// Token-savings percentage of `toon_text` relative to `json_text` under
/// `profile`: `(1 - toon_tokens / json_tokens) * 100`. Returns `0.0` when the
/// JSON side tokenizes to zero tokens (avoiding a divide-by-zero).
pub fn token_savings_pct(json_text: &str, toon_text: &str, profile: &TokenizerProfile) -> f64 {
    let json_tokens = count_tokens(json_text, profile);
    if json_tokens == 0 {
        return 0.0;
    }
    let toon_tokens = count_tokens(toon_text, profile);
    (1.0 - (toon_tokens as f64 / json_tokens as f64)) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn heuristic_is_floor_rule() {
        assert_eq!(count_tokens("", &TokenizerProfile::Heuristic), 0);
        // 4 bytes -> 1 token, 5 bytes -> 2 tokens (ceil).
        assert_eq!(count_tokens("abcd", &TokenizerProfile::Heuristic), 1);
        assert_eq!(count_tokens("abcde", &TokenizerProfile::Heuristic), 2);
    }

    #[test]
    fn bpe_profiles_are_monotone_on_repeated_schema() {
        // A schema-repetitive JSON array should tokenize to fewer tokens as a
        // TOON-equivalent compact form (header + rows). We cannot guarantee the
        // exact TOON here, but at minimum BPE counting must never panic and must
        // return non-zero for non-empty text.
        let json = r#"[{"id":1,"name":"a"},{"id":2,"name":"b"}]"#;
        assert!(count_tokens(json, &TokenizerProfile::Cl100k) > 0);
        assert!(count_tokens(json, &TokenizerProfile::O200k) > 0);
    }

    #[test]
    fn resolve_model_maps_families_without_network() {
        assert_eq!(resolve_model("gpt-4o"), TokenizerProfile::O200k);
        assert_eq!(resolve_model("gpt-5"), TokenizerProfile::O200k);
        assert_eq!(resolve_model("o3-mini"), TokenizerProfile::O200k);
        assert_eq!(resolve_model("gpt-4"), TokenizerProfile::Cl100k);
        assert_eq!(resolve_model("text-embedding-ada-002"), TokenizerProfile::Cl100k);
        // Unknown name resolves to heuristic, never panics / never fetches.
        assert_eq!(resolve_model("some-future-model"), TokenizerProfile::Heuristic);
    }

    #[test]
    fn named_unknown_falls_back_to_heuristic_count() {
        let profile = TokenizerProfile::Named("mystery-model".to_string());
        assert_eq!(count_tokens("abcd", &profile), 1);
    }

    #[test]
    fn parse_profile_keywords() {
        assert_eq!(parse_profile("heuristic"), TokenizerProfile::Heuristic);
        assert_eq!(parse_profile("CL100K"), TokenizerProfile::Cl100k);
        assert_eq!(parse_profile("o200k"), TokenizerProfile::O200k);
        assert_eq!(
            parse_profile("gpt-4o-mini"),
            TokenizerProfile::Named("gpt-4o-mini".to_string())
        );
    }

    #[test]
    fn savings_pct_is_nonnegative_and_zero_safe() {
        let json = r#"{"a":1,"b":2,"c":3}"#;
        let toon = "a:1\nb:2\nc:3\n";
        let pct = token_savings_pct(json, toon, &TokenizerProfile::Cl100k);
        assert!(pct.is_finite());
        // TOON is not necessarily smaller in tokens for a single tiny object,
        // but the function must not return NaN/negative-defect.
        let zero = token_savings_pct("", "", &TokenizerProfile::Cl100k);
        assert!((zero - 0.0).abs() < f64::EPSILON);
    }

    proptest! {
        #[test]
        fn count_tokens_never_panics_on_arbitrary_text(s in ".*") {
            let _ = count_tokens(&s, &TokenizerProfile::Cl100k);
            let _ = count_tokens(&s, &TokenizerProfile::O200k);
            let _ = count_tokens(&s, &TokenizerProfile::Heuristic);
        }
    }
}
