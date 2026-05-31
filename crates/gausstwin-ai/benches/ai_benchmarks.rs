use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gausstwin_ai::ml::models::{gnn::GNNModel, timeseries::TimeSeriesModel};
use ndarray::Array2;

fn bench_gnn(c: &mut Criterion) {
    let mut group = c.benchmark_group("gnn");
    let model = GNNModel::new();
    let input = Array2::random((10, 10));

    group.bench_function("forward_pass", |b| {
        b.iter(|| {
            model.forward(black_box(&input));
        });
    });

    group.finish();
}

fn bench_timeseries(c: &mut Criterion) {
    let mut group = c.benchmark_group("timeseries");
    let model = TimeSeriesModel::new();
    let input = Array2::random((100, 5));

    group.bench_function("predict", |b| {
        b.iter(|| {
            model.predict(black_box(&input));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_gnn, bench_timeseries);
criterion_main!(benches);
