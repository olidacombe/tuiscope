use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fakeit::beer;
use std::collections::HashMap;
use tuiscope::FuzzyFinder;

fn set_filter(c: &mut Criterion) {
    let mut options = HashMap::<u32, String>::new();
    for n in 1..1_000_000 {
        options.insert(n, beer::name());
    }
    let mut fuzzy_finder = FuzzyFinder::new(&options);
    c.bench_function("score 1,000,000", |b| {
        b.iter(|| {
            fuzzy_finder.set_filter(black_box("b".to_string()));
        })
    });
}

criterion_group!(benches, set_filter);
criterion_main!(benches);
