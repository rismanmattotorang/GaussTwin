use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gausstwin_cosim::common::time::TimeManager;

fn time_management_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("time_management");

    group.bench_function("basic_time_advance", |b| {
        let mut time_mgr = TimeManager::new();
        b.iter(|| {
            time_mgr.advance(black_box(0.1));
        });
    });

    group.finish();
}

criterion_group!(benches, time_management_benchmark);
criterion_main!(benches);
