// SPDX-License-Identifier: AGPL-3.0-only
#![allow(clippy::unwrap_used, clippy::disallowed_methods, clippy::cast_lossless)]
use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Instant;

use tooned_parse::exceeds_max_structural_depth;
use tooned_toon::{
    apply_dict, decode_toon_with_options, encode_toon_raw_with_options, expand_legend,
};
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
    let value = tooned_json::parse_json(&payload).unwrap();
    let encoded = encode_toon_raw_with_options(&value, &opts).unwrap();
    let dict = apply_dict(&encoded, &[]).unwrap_or(encoded);

    let n = 200;
    let mut t_expand = 0u128;
    let mut t_depth = 0u128;
    let mut t_decode = 0u128;

    for _ in 0..n {
        let a = Instant::now();
        let plain = expand_legend(&dict, opts.max_input_bytes).unwrap();
        t_expand += a.elapsed().as_nanos();

        let b = Instant::now();
        let _ = exceeds_max_structural_depth(plain.as_bytes());
        t_depth += b.elapsed().as_nanos();

        let c = Instant::now();
        let _ = decode_toon_with_options(&dict, &opts).unwrap();
        t_decode += c.elapsed().as_nanos();

        black_box(&plain);
    }
    let ms = |ns: u128| format!("{:.3} ms", ns as f64 / f64::from(n) / 1e6);
    println!("expand_legend: {}", ms(t_expand));
    println!("exceed_depth : {}", ms(t_depth));
    println!("decode_full  : {}", ms(t_decode));
}
