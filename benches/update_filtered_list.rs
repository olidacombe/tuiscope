use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fakeit::beer;
use tuiscope::FuzzyFinder;

fn set_filter(c: &mut Criterion) {
    let mut options = Vec::<String>::new();
    for _ in 1..1_000_000 {
        options.push(beer::name());
    }
    let mut fuzzy_finder = FuzzyFinder::default();
    fuzzy_finder.push_options(&options);
    c.bench_function("score 1,000,000", |b| {
        b.iter(|| {
            fuzzy_finder.set_filter(black_box("b".to_string()));
        })
    });
}

criterion_group!(benches, set_filter);
criterion_main!(benches);
