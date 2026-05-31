use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gausstwin_core::space::{
    continuous::ContinuousSpace, graph::GraphSpace, grid::GridSpace, Position, Space,
};
use rand::Rng;

fn create_grid_space() -> GridSpace {
    GridSpace::new(vec![100, 100], vec![true, true])
}

fn create_continuous_space() -> ContinuousSpace {
    ContinuousSpace::new(vec![(0.0, 100.0), (0.0, 100.0)], vec![true, true])
}

fn create_graph_space() -> GraphSpace {
    let mut space = GraphSpace::new(false);
    for _ in 0..100 {
        space.add_node();
    }
    for i in 0..99 {
        space.add_edge(i.into(), (i + 1).into());
    }
    space
}

fn bench_grid_space(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_space");
    let mut space = create_grid_space();
    let mut rng = rand::thread_rng();

    group.bench_function("add_agent", |b| {
        b.iter(|| {
            let pos = Position::Grid(vec![rng.gen_range(0..100), rng.gen_range(0..100)]);
            space.add_agent(black_box(rng.gen()), black_box(pos));
        });
    });

    group.bench_function("get_neighbors", |b| {
        let pos = Position::Grid(vec![50, 50]);
        b.iter(|| {
            space.get_neighbors(black_box(&pos), black_box(5.0));
        });
    });

    group.finish();
}

fn bench_continuous_space(c: &mut Criterion) {
    let mut group = c.benchmark_group("continuous_space");
    let mut space = create_continuous_space();
    let mut rng = rand::thread_rng();

    group.bench_function("add_agent", |b| {
        b.iter(|| {
            let pos =
                Position::Continuous(vec![rng.gen_range(0.0..100.0), rng.gen_range(0.0..100.0)]);
            space.add_agent(black_box(rng.gen()), black_box(pos));
        });
    });

    group.bench_function("get_neighbors", |b| {
        let pos = Position::Continuous(vec![50.0, 50.0]);
        b.iter(|| {
            space.get_neighbors(black_box(&pos), black_box(5.0));
        });
    });

    group.finish();
}

fn bench_graph_space(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_space");
    let mut space = create_graph_space();
    let mut rng = rand::thread_rng();

    group.bench_function("add_agent", |b| {
        b.iter(|| {
            let pos = Position::Graph(rng.gen_range(0..100));
            space.add_agent(black_box(rng.gen()), black_box(pos));
        });
    });

    group.bench_function("get_neighbors", |b| {
        let pos = Position::Graph(50);
        b.iter(|| {
            space.get_neighbors(black_box(&pos), black_box(2.0));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_grid_space,
    bench_continuous_space,
    bench_graph_space
);
criterion_main!(benches);
