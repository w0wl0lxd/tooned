// SPDX-License-Identifier: AGPL-3.0-only

//! Comprehensive Criterion benchmarks for the conversion hot path.
//!
//! Covers end-to-end `tooned_core::maybe_tooned` across multiple doctypes,
//! payload sizes, and `ConversionOptions` variants. All payloads are generated
//! in-process so the benchmarks are self-contained and deterministic.

use std::fmt::Write as _;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use tooned_core::{ConversionOptions, maybe_tooned};

// --- payload generators ------------------------------------------------------

fn uniform_json(rows: usize) -> Vec<u8> {
    let mut s = String::from("[");
    for i in 0..rows {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, r#"{{"id":{i},"name":"row-{i}","active":true,"score":{i}.5}}"#);
    }
    s.push(']');
    s.into_bytes()
}

fn uniform_xml(rows: usize) -> Vec<u8> {
    let mut s = String::from("<?xml version=\"1.0\"?>\n<data>");
    for i in 0..rows {
        let _ = write!(s, r#"<record id="{i}" name="row-{i}" active="true" score="{i}" />"#);
    }
    s.push_str("</data>");
    s.into_bytes()
}

fn uniform_ndjson(rows: usize) -> Vec<u8> {
    let mut s = String::new();
    for i in 0..rows {
        let _ = writeln!(s, r#"{{"id":{i},"name":"row-{i}","active":true,"score":{i}.5}}"#);
    }
    s.into_bytes()
}

fn uniform_csv(rows: usize) -> Vec<u8> {
    let mut s = String::from("id,name,active,score\n");
    for i in 0..rows {
        let _ = writeln!(s, "{i},row-{i},true,{i}.5");
    }
    s.into_bytes()
}

fn uniform_tsv(rows: usize) -> Vec<u8> {
    let mut s = String::from("id\tname\tactive\tscore\n");
    for i in 0..rows {
        let _ = writeln!(s, "{i}\trow-{i}\ttrue\t{i}.5");
    }
    s.into_bytes()
}

fn toml_tables(rows: usize) -> Vec<u8> {
    let mut s = String::new();
    for i in 0..rows {
        let _ = writeln!(s, "[[item]]");
        let _ = writeln!(s, "id = {i}");
        let _ = writeln!(s, "name = \"row-{i}\"");
        let _ = writeln!(s, "active = true");
        let _ = writeln!(s, "score = {i}.5");
    }
    s.into_bytes()
}

fn yaml_list(rows: usize) -> Vec<u8> {
    let mut s = String::new();
    for i in 0..rows {
        let _ = writeln!(s, "- id: {i}");
        let _ = writeln!(s, "  name: row-{i}");
        let _ = writeln!(s, "  active: true");
        let _ = writeln!(s, "  score: {i}.5");
    }
    s.into_bytes()
}

fn matrix_json(rows: usize, cols: usize) -> Vec<u8> {
    let mut s = String::from("[");
    for r in 0..rows {
        if r > 0 {
            s.push(',');
        }
        s.push('[');
        for c in 0..cols {
            if c > 0 {
                s.push(',');
            }
            let _ = write!(s, "{}", r * cols + c);
        }
        s.push(']');
    }
    s.push(']');
    s.into_bytes()
}

fn mixed_schema_json(rows: usize) -> Vec<u8> {
    let mut s = String::from("[");
    for i in 0..rows {
        if i > 0 {
            s.push(',');
        }
        if i % 3 == 0 {
            let _ = write!(s, r#"{{"id":{i},"name":"row-{i}"}}"#);
        } else if i % 3 == 1 {
            let _ = write!(s, r#"{{"id":{i},"status":"ok","score":{i}.5}}"#);
        } else {
            let _ = write!(s, r#"{{"id":{i},"tags":["a","b","c"]}}"#);
        }
    }
    s.push(']');
    s.into_bytes()
}

fn nested_object(depth: usize) -> Vec<u8> {
    let mut s = String::new();
    let _ = write!(s, "{{\"user\":");
    for _ in 0..depth {
        let _ = write!(s, "{{\"profile\":");
    }
    let _ = write!(s, r#"{{"name":"Alice","age":30,"tags":["x","y"]}}"#);
    for _ in 0..depth {
        let _ = write!(s, "}}");
    }
    let _ = write!(s, "}}");
    s.into_bytes()
}

fn small_json() -> Vec<u8> {
    br#"{"id":1,"name":"row-1","active":true,"score":1.5}"#.to_vec()
}

// --- benchmarks --------------------------------------------------------------

fn bench_formats_100kib(c: &mut Criterion) {
    let payloads: Vec<(&str, Vec<u8>)> = vec![
        ("json", uniform_json(1750)),
        ("ndjson", uniform_ndjson(1750)),
        ("csv", uniform_csv(3500)),
        ("tsv", uniform_tsv(3500)),
        ("xml", uniform_xml(1650)),
        ("toml", toml_tables(2500)),
        ("yaml", yaml_list(2500)),
    ];

    let mut group = c.benchmark_group("conversion_100kib");
    for (name, payload) in payloads {
        group.throughput(Throughput::Bytes(payload.len() as u64));
        group.bench_with_input(BenchmarkId::new("maybe_tooned", name), &payload, |b, p| {
            let opts = ConversionOptions::default();
            b.iter(|| maybe_tooned(black_box(p), black_box(&opts)));
        });
    }
    group.finish();
}

fn bench_json_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_sizes");
    for rows in [100, 500, 1750, 5000, 10_000] {
        let payload = uniform_json(rows);
        group.throughput(Throughput::Bytes(payload.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(rows), &payload, |b, p| {
            let opts = ConversionOptions::default();
            b.iter(|| maybe_tooned(black_box(p), black_box(&opts)));
        });
    }
    group.finish();
}

fn bench_json_options(c: &mut Criterion) {
    let payload = uniform_json(1750);

    let mut group = c.benchmark_group("json_options");
    group.throughput(Throughput::Bytes(payload.len() as u64));

    let default_opts = ConversionOptions::default();
    group.bench_with_input(BenchmarkId::new("default", ""), &payload, |b, p| {
        b.iter(|| maybe_tooned(black_box(p), black_box(&default_opts)));
    });

    let no_dict = ConversionOptions { dict_enabled: false, ..ConversionOptions::default() };
    group.bench_with_input(BenchmarkId::new("no_dict", ""), &payload, |b, p| {
        b.iter(|| maybe_tooned(black_box(p), black_box(&no_dict)));
    });

    let cli_like = ConversionOptions {
        dict_enabled: true,
        auto_margin: true,
        entropy_gate: true,
        ..ConversionOptions::default()
    };
    group.bench_with_input(BenchmarkId::new("cli_like", ""), &payload, |b, p| {
        b.iter(|| maybe_tooned(black_box(p), black_box(&cli_like)));
    });

    let cache_stable = ConversionOptions { cache_stable: true, ..ConversionOptions::default() };
    group.bench_with_input(BenchmarkId::new("cache_stable", ""), &payload, |b, p| {
        b.iter(|| maybe_tooned(black_box(p), black_box(&cache_stable)));
    });

    group.finish();
}

fn bench_challenging_shapes(c: &mut Criterion) {
    let mut group = c.benchmark_group("challenging_shapes");

    let matrix = matrix_json(100, 20);
    group.throughput(Throughput::Bytes(matrix.len() as u64));
    group.bench_with_input(BenchmarkId::new("matrix_json", ""), &matrix, |b, p| {
        let opts = ConversionOptions::default();
        b.iter(|| maybe_tooned(black_box(p), black_box(&opts)));
    });

    let mixed = mixed_schema_json(1000);
    group.throughput(Throughput::Bytes(mixed.len() as u64));
    group.bench_with_input(BenchmarkId::new("mixed_schema_json", ""), &mixed, |b, p| {
        let opts = ConversionOptions::default();
        b.iter(|| maybe_tooned(black_box(p), black_box(&opts)));
    });

    let nested = nested_object(6);
    group.throughput(Throughput::Bytes(nested.len() as u64));
    group.bench_with_input(BenchmarkId::new("nested_object", ""), &nested, |b, p| {
        let opts = ConversionOptions::default();
        b.iter(|| maybe_tooned(black_box(p), black_box(&opts)));
    });

    let small = small_json();
    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_with_input(BenchmarkId::new("small_json", ""), &small, |b, p| {
        let opts = ConversionOptions::default();
        b.iter(|| maybe_tooned(black_box(p), black_box(&opts)));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_formats_100kib,
    bench_json_sizes,
    bench_json_options,
    bench_challenging_shapes
);
criterion_main!(benches);
