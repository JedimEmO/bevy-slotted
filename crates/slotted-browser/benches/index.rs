//! Index build and query benchmarks.
//!
//! Acceptance (contract section 4): a 50k-entry index builds under one second
//! in release. `index_build_50k` is the measurement that backs it; the
//! substring index is benchmarked on its own because it dominates that cost.

use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use slotted_browser::index::build::BuildInputs;
use slotted_browser::index::{BrowserIndex, SubstringIndex};
use slotted_browser::{Categories, IngredientTypes, RecipeStore, Subtypes};
use slotted_model::Namespaced;
use slotted_registry::defs::{ItemDef, TagDef, TagEntry};
use slotted_registry::registry::Registries;

fn id(s: &str) -> Namespaced {
    Namespaced::parse(s).expect("generated ids are well formed")
}

/// `n` items across ten namespaces and ten tags.
fn synthetic(n: u32) -> BuildInputs {
    let mut registries = Registries::new();
    let mut tags: Vec<(Namespaced, Vec<TagEntry>)> = (0..10)
        .map(|t| (id(&format!("c:group_{t}")), Vec::new()))
        .collect();
    for i in 0..n {
        let name = format!("mod{}:item_{i}", i % 10);
        let mut def = ItemDef::new(id(&name));
        def.display_name = Some(format!("Synthetic Item {i}"));
        let tag = usize::try_from(i % 10).unwrap_or(0);
        def.tags = vec![tags[tag].0.clone()];
        tags[tag].1.push(TagEntry::Item(id(&name)));
        registries.add_item(def).expect("generated ids are unique");
    }
    for (name, values) in tags {
        registries.add_tag(TagDef {
            name,
            values,
            replace: false,
        });
    }
    let (frozen, _) = registries.freeze().expect("the synthetic set freezes");
    let registries = Arc::new(frozen);
    let categories = Categories::default();
    let recipes = RecipeStore::build(&registries, &categories);
    BuildInputs {
        registries,
        types: IngredientTypes::with_builtins(),
        subtypes: Subtypes::default(),
        categories,
        recipes,
    }
}

fn substring_index(c: &mut Criterion) {
    let texts: Vec<(u32, String)> = (0..50_000u32)
        .map(|i| (i, format!("item {i} of the synthetic set")))
        .collect();
    c.bench_function("substring_index_build_50k", |b| {
        b.iter(|| SubstringIndex::build(texts.len(), texts.iter().cloned()));
    });
}

fn index_build(c: &mut Criterion) {
    let inputs = synthetic(50_000);
    let mut group = c.benchmark_group("index_build");
    group.sample_size(10);
    group.bench_function("index_build_50k", |b| {
        b.iter(|| BrowserIndex::build(&inputs));
    });
    group.finish();
}

fn index_query(c: &mut Criterion) {
    let index = BrowserIndex::build(&synthetic(50_000));
    let config = slotted_browser::SearchConfig::default();
    let hidden = slotted_browser::index::Bitset::new(index.len());
    c.bench_function("index_query_50k", |b| {
        b.iter(|| {
            let tokens = slotted_browser::tokenize("synthetic 4|5 -@mod3", &config);
            slotted_browser::evaluate(&index, &tokens, &config, &hidden)
        });
    });
}

criterion_group!(benches, substring_index, index_build, index_query);
criterion_main!(benches);
