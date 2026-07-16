// SPDX-License-Identifier: AGPL-3.0-only

//! `--ignored` latency guardrail test (T077, companion to
//! `crates/tooned-cli/benches/hot_path.rs`'s criterion benchmark).
//!
//! Asserts `tooned_core::maybe_tooned` completes in low single-digit
//! milliseconds for a ~100 KiB uniform-array JSON payload (constitution
//! Technology Constraints: "target: <5ms at 100 KiB"), rather than merely
//! reporting the number the way a criterion benchmark does.
//!
//! This lives in `tests/`, not in `benches/hot_path.rs` itself: that bench
//! target is registered with `harness = false` (required by
//! `criterion_main!`, which defines its own `fn main`), so a
//! `#[test]`-attributed function placed there is never actually picked up
//! by `cargo test`/`--ignored` filtering -- `cargo test`/`cargo bench` on a
//! `harness = false` target just invokes that generated `main` directly,
//! bypassing the standard libtest harness entirely (verified empirically).
//!
//! `#[ignore]`d by default (like the constitution specifies) because
//! absolute wall-clock latency is inherently machine/CI-dependent noise;
//! run explicitly via `cargo test --release -p tooned-cli \
//! --test hot_path_latency -- --ignored`.

use std::fmt::Write as _;
use std::time::Instant;

use tooned_core::{ConversionOptions, maybe_tooned};

/// ~97.6 KiB uniform array of objects -- matches
/// `benches/hot_path.rs`'s fixture (kept in sync deliberately; duplicated
/// rather than shared, per this workspace's existing `tests/` convention of
/// self-contained fixtures per test binary).
fn uniform_array_json_100kib() -> Vec<u8> {
    let mut s = String::from("[");
    for i in 0..1750 {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, r#"{{"id":{i},"name":"row-{i}","active":true,"score":{i}.5}}"#);
    }
    s.push(']');
    s.into_bytes()
}

#[test]
#[ignore = "wall-clock latency guardrail -- run explicitly, not part of the default fast test suite"]
fn maybe_tooned_completes_in_low_single_digit_milliseconds_at_100kib() {
    const ITERATIONS: u32 = 50;

    let payload = uniform_array_json_100kib();
    assert!(
        (95_000..=105_000).contains(&payload.len()),
        "fixture must actually be ~100 KiB for this guardrail to be meaningful, got {} bytes",
        payload.len()
    );
    let opts = ConversionOptions::default();

    // Warm up (page cache, allocator, branch predictor) before timing --
    // the guardrail cares about steady-state hot-path latency, not
    // first-call cold-start cost.
    for _ in 0..10 {
        let _ = maybe_tooned(&payload, &opts);
    }

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let result = maybe_tooned(&payload, &opts);
        assert!(result.is_ok(), "maybe_tooned must be infallible for payload-driven input");
    }
    let elapsed = start.elapsed();
    let avg = elapsed / ITERATIONS;

    // The constitution's stated target is <5ms (release/optimized
    // profile); this guardrail asserts a deliberately more generous <10ms
    // there to absorb shared-CI/virtualized timing noise while still
    // catching a genuine multi-x regression (e.g. an accidental O(n^2)
    // path or newly introduced blocking I/O). An unoptimized debug build
    // is not representative of the real latency claim at all (measured
    // ~10x+ slower in practice), so it gets a much more permissive bound
    // instead of being silently skipped -- still catches a catastrophic
    // regression, just not a modest one. Run with `--release` for the
    // real guardrail: `cargo test --release -p tooned-cli \
    // --test hot_path_latency -- --ignored`.
    let threshold_ms: u128 = if cfg!(debug_assertions) { 200 } else { 10 };
    assert!(
        avg.as_millis() < threshold_ms,
        "maybe_tooned averaged {avg:?} per call over {ITERATIONS} iterations on a ~{} byte \
         uniform-array payload -- expected under {threshold_ms}ms ({})",
        payload.len(),
        if cfg!(debug_assertions) {
            "debug build; run with --release for the real <10ms guardrail"
        } else {
            "release build"
        }
    );
}
