// SPDX-License-Identifier: AGPL-3.0-only

//! Component-level Criterion benchmarks for the dictionary tier.

use std::fmt::Write as _;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use tooned_toon::{apply_dict, expand_legend};

fn uniform_table(rows: usize, repeated: &str) -> String {
    let mut s = String::from("[N]{id,name,role}:\n\n");
    for i in 0..rows {
        let _ = writeln!(s, "  {i},row-{i},{repeated}");
    }
    s
}

fn object_document(repeated: &str) -> String {
    format!("a: 1\nb: {repeated}\nc: {repeated}\n")
}

fn bench_apply_dict(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply_dict");

    let compressible_table = uniform_table(1000, "this_is_a_very_long_repeated_value");
    group.throughput(Throughput::Bytes(compressible_table.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("compressible_table", "1000"),
        &compressible_table,
        |b, toon| {
            let protected: Vec<String> = vec![];
            b.iter(|| apply_dict(black_box(toon), black_box(&protected)));
        },
    );

    let incompressible_table = uniform_table(1000, "x");
    group.throughput(Throughput::Bytes(incompressible_table.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("incompressible_table", "1000"),
        &incompressible_table,
        |b, toon| {
            let protected: Vec<String> = vec![];
            b.iter(|| apply_dict(black_box(toon), black_box(&protected)));
        },
    );

    let compressible_object = object_document("this_is_a_very_long_repeated_value");
    group.throughput(Throughput::Bytes(compressible_object.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("compressible_object", ""),
        &compressible_object,
        |b, toon| {
            let protected: Vec<String> = vec![];
            b.iter(|| apply_dict(black_box(toon), black_box(&protected)));
        },
    );

    let protected_table = uniform_table(1000, "this_is_a_very_long_repeated_value");
    group.throughput(Throughput::Bytes(protected_table.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("protected_table", "1000"),
        &protected_table,
        |b, toon| {
            let protected: Vec<String> = vec!["role".to_string()];
            b.iter(|| apply_dict(black_box(toon), black_box(&protected)));
        },
    );

    group.finish();
}

fn bench_expand_legend(c: &mut Criterion) {
    let Some(toon) = apply_dict(&uniform_table(1000, "this_is_a_very_long_repeated_value"), &[])
    else {
        return;
    };
    let mut group = c.benchmark_group("expand_legend");
    group.throughput(Throughput::Bytes(toon.len() as u64));
    group.bench_function("expand_legend", |b| {
        b.iter(|| expand_legend(black_box(&toon), black_box(usize::MAX)));
    });
    group.finish();
}

fn bench_expand_legend_object(c: &mut Criterion) {
    let Some(toon) = apply_dict(&object_document("this_is_a_very_long_repeated_value"), &[]) else {
        return;
    };
    let mut group = c.benchmark_group("expand_legend_object");
    group.throughput(Throughput::Bytes(toon.len() as u64));
    group.bench_function("expand_legend_object", |b| {
        b.iter(|| expand_legend(black_box(&toon), black_box(usize::MAX)));
    });
    group.finish();
}

criterion_group!(benches, bench_apply_dict, bench_expand_legend, bench_expand_legend_object);
criterion_main!(benches);
