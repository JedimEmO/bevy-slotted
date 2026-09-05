//! Index build and query benchmarks. Acceptance: 50k synthetic entries build
//! under one second in release. PHASE3-IMPL: A fills the synthetic registry.

use criterion::{Criterion, criterion_group, criterion_main};
use slotted_browser::index::SubstringIndex;

fn substring_index(c: &mut Criterion) {
    let texts: Vec<(u32, String)> = (0..50_000u32)
        .map(|i| (i, format!("item {i} of the synthetic set")))
        .collect();
    c.bench_function("substring_index_build_50k", |b| {
        b.iter(|| SubstringIndex::build(texts.len(), texts.iter().cloned()));
    });
}

criterion_group!(benches, substring_index);
criterion_main!(benches);
