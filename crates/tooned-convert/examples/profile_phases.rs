// SPDX-License-Identifier: AGPL-3.0-only
#![allow(clippy::unwrap_used, clippy::disallowed_methods, clippy::cast_lossless)]
use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Instant;

use tooned_convert::parse_to_value;
use tooned_detect::detect;
use tooned_toon::{apply_dict, decode_toon_with_options, encode_toon_raw_with_options};
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

    let mut t_detect = 0u128;
    let mut t_parse = 0u128;
    let mut t_encode = 0u128;
    let mut t_dict = 0u128;
    let mut t_decode = 0u128;
    let mut total = 0u128;

    for _ in 0..n {
        let tot = Instant::now();
        let d = Instant::now();
        let dt = detect(&payload, opts.format_hint);
        t_detect += d.elapsed().as_nanos();
        let dt = dt.unwrap();
        let p = Instant::now();
        let value = tooned_json::parse_json(&payload).unwrap();
        t_parse += p.elapsed().as_nanos();

        let e = Instant::now();
        let encoded = encode_toon_raw_with_options(&value, &opts).unwrap();
        t_encode += e.elapsed().as_nanos();

        let di = Instant::now();
        let protected = Vec::new();
        let dict = apply_dict(&encoded, &protected).unwrap_or(encoded);
        t_dict += di.elapsed().as_nanos();

        let dc = Instant::now();
        let _ = decode_toon_with_options(&dict, &opts).unwrap();
        t_decode += dc.elapsed().as_nanos();

        total += tot.elapsed().as_nanos();
        black_box((dt, &value));
    }

    let ms = |ns: u128| format!("{:.3} ms", ns as f64 / f64::from(n) / 1e6);
    println!("total   : {}", ms(total));
    println!("detect  : {}", ms(t_detect));
    println!("parse   : {}", ms(t_parse));
    println!("encode  : {}", ms(t_encode));
    println!("dict    : {}", ms(t_dict));
    println!("decode  : {}", ms(t_decode));
    let _ = parse_to_value(&payload, None);
}
