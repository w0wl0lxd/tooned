// SPDX-License-Identifier: AGPL-3.0-only
#![allow(clippy::unwrap_used, clippy::cast_lossless)]
use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Instant;
use tooned_convert::maybe_tooned;
use tooned_types::ConversionOptions;

fn uniform_json() -> Vec<u8> {
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

fn main() {
    let payload = uniform_json();
    let opts = ConversionOptions::default();
    let n = 200;
    // warmup
    for _ in 0..10 {
        let _ = maybe_tooned(&payload, &opts);
    }
    let mut t = 0u128;
    for _ in 0..n {
        let start = Instant::now();
        let r = maybe_tooned(&payload, &opts).unwrap();
        t += start.elapsed().as_nanos();
        black_box(&r);
    }
    println!("maybe_tooned total: {:.3} ms", t as f64 / f64::from(n) / 1e6);
}
