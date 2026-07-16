//! Criterion benchmark for the conversion hot path (T077).
//!
//! Benchmarks `tooned_core::maybe_tooned` against a ~100 KiB uniform
//! array-of-objects JSON payload -- the shape TOON's tabular encoding
//! actually wins on, and the payload shape the constitution's Technology
//! Constraints section names for its `<5ms` latency budget.
//!
//! The `--ignored` latency guardrail test that actually *asserts* the
//! `<5ms` budget (rather than just reporting it) lives in
//! `crates/tooned-cli/tests/hot_path_latency.rs`, not here: this target is
//! registered with `harness = false` (required by `criterion_main!`, which
//! defines its own `fn main`), so a `#[test]`-attributed function placed in
//! this same file would never actually be picked up by `cargo test` --
//! `cargo test`/`cargo bench` on a `harness = false` target simply invokes
//! that generated `main` directly, bypassing the standard libtest harness
//! (and hence `--ignored` filtering) entirely. Verified empirically, not
//! assumed.

use std::fmt::Write as _;
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use tooned_core::{ConversionOptions, maybe_tooned};

/// A uniform array of objects sized to land at roughly 100 KiB of JSON --
/// the exact payload shape/size the constitution's latency budget names.
fn uniform_array_json_100kib() -> Vec<u8> {
    // Each row is ~57 bytes of JSON (`{"id":N,"name":"row-N","active":true,"score":N.5},`);
    // 1750 rows lands at ~97.6 KiB.
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

/// A record-list XML payload sized to land at roughly 100 KiB. Mirrors the
/// JSON fixture: repeated `<record>` elements with a consistent set of
/// attributes, the shape the XML parser produces a uniform array from.
fn uniform_array_xml_100kib() -> Vec<u8> {
    // 1650 rows lands at ~100.6 KiB for this attribute set.
    let mut s = String::from("<?xml version=\"1.0\"?>\n<data>");
    for i in 0..1650 {
        let _ = write!(s, r#"<record id="{i}" name="row-{i}" active="true" score="{i}" />"#);
    }
    s.push_str("</data>");
    s.into_bytes()
}

fn bench_maybe_tooned_uniform_array_100kib(c: &mut Criterion) {
    let payload = uniform_array_json_100kib();
    let opts = ConversionOptions::default();

    c.bench_function("maybe_tooned_uniform_array_100kib", |b| {
        b.iter(|| maybe_tooned(black_box(&payload), black_box(&opts)));
    });
}

fn bench_maybe_tooned_uniform_xml_100kib(c: &mut Criterion) {
    let payload = uniform_array_xml_100kib();
    let opts = ConversionOptions::default();

    c.bench_function("maybe_tooned_uniform_xml_100kib", |b| {
        b.iter(|| maybe_tooned(black_box(&payload), black_box(&opts)));
    });
}

criterion_group!(
    benches,
    bench_maybe_tooned_uniform_array_100kib,
    bench_maybe_tooned_uniform_xml_100kib
);
criterion_main!(benches);
