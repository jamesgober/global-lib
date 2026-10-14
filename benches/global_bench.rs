//! Benchmarks for reads and writes of named and typed globals.

#![allow(missing_docs)]

use std::hint::black_box;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use criterion::{Criterion, criterion_group, criterion_main};
use global_lib::{Global, Globals};

static TYPED: Global<u64> = Global::new();

fn bench_reads(c: &mut Criterion) {
    let g = Globals::new();
    for i in 0..32u64 {
        g.set(&format!("key_{i}"), i);
    }
    let _previous = TYPED.set(7);
    let mut grp = c.benchmark_group("read");
    grp.bench_function("named_with", |b| {
        b.iter(|| g.with::<u64, _>(black_box("key_16"), |v| *v));
    });
    grp.bench_function("named_value", |b| {
        b.iter(|| g.value::<u64>(black_box("key_16")));
    });
    grp.bench_function("named_get_arc", |b| {
        b.iter(|| g.get::<u64>(black_box("key_16")));
    });
    grp.bench_function("named_miss", |b| {
        b.iter(|| g.value::<u64>(black_box("absent")));
    });
    grp.bench_function("typed_with", |b| b.iter(|| TYPED.with(|v| *v)));
    grp.bench_function("typed_value", |b| b.iter(|| TYPED.value()));
    grp.finish();
}

fn bench_writes(c: &mut Criterion) {
    let g = Globals::new();
    for i in 0..32u64 {
        g.set(&format!("key_{i}"), i);
    }
    let mut grp = c.benchmark_group("write");
    grp.bench_function("named_set_32_entries", |b| {
        b.iter(|| g.set(black_box("key_16"), 1u64));
    });
    grp.bench_function("named_edit_32_entries", |b| {
        b.iter(|| g.edit::<u64>(black_box("key_16"), |v| *v += 1));
    });
    grp.bench_function("typed_set", |b| b.iter(|| TYPED.set(black_box(9))));
    grp.finish();
}

fn bench_contended_reads(c: &mut Criterion) {
    let g = Arc::new(Globals::new());
    g.set("hot", 42u64);
    let mut grp = c.benchmark_group("contended_read");
    for threads in [1usize, 4, 16] {
        grp.bench_function(format!("named_with_{threads}_threads"), |b| {
            b.iter_custom(|iters| {
                let per = iters / threads as u64;
                let start = Instant::now();
                thread::scope(|s| {
                    for _ in 0..threads {
                        let g = Arc::clone(&g);
                        let _handle = s.spawn(move || {
                            for _ in 0..per {
                                let _ = black_box(g.with::<u64, _>("hot", |v| *v));
                            }
                        });
                    }
                });
                start.elapsed()
            });
        });
    }
    grp.finish();
}

criterion_group!(benches, bench_reads, bench_writes, bench_contended_reads);
criterion_main!(benches);
